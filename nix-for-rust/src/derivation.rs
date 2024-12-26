use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use crate::store::{NixStore, NixStorePath};
use anyhow::Result;
use nom::bytes::complete::{escaped, tag};
use nom::combinator::{fail, opt, value};
use nom::error::VerboseError;
use nom::multi::separated_list0;
use nom::branch::alt;
use nom::character::complete::{char, none_of, one_of};
use nom::sequence::delimited;
use nom::{Finish, IResult, Parser};

type ParseRes<'s, T> = IResult<&'s str, T, VerboseError<&'s str>>;

#[derive(Debug)]
enum HashAlgorithm {
  Md5,
  Sha1,
  Sha256,
  Sha512
}

impl HashAlgorithm {
  fn parse(s: &str) -> Option<Self> {
    match s {
      "md5" => Some(HashAlgorithm::Md5),
      "sha1" => Some(HashAlgorithm::Sha1),
      "sha256" => Some(HashAlgorithm::Sha256),
      "sha512" => Some(HashAlgorithm::Sha512),
      _ => None
    }
  }
}

#[derive(Debug)]
enum ContentAddressedMethod {
  NixArchive,
  Git,
  Text,
  Flat
}

impl ContentAddressedMethod {
  fn parse(s: &str) -> (&str, Self) {
    if let Some(s) = s.strip_prefix("r:") {
      (s, ContentAddressedMethod::NixArchive)
    } else if let Some(s) = s.strip_prefix("git:") {
      (s, ContentAddressedMethod::Git)
    } else if let Some(s) = s.strip_prefix("text:") {
      (s, ContentAddressedMethod::Text)
    } else {
      (s, ContentAddressedMethod::Flat)
    }
  } 
}

#[derive(Debug)]
enum DerivationOutput {
  Deferred,
  InputAddressed {
    path: NixStorePath,
  },
  Impure {
    method: ContentAddressedMethod,
    hash_algo: HashAlgorithm
  },
  CAFixed {
    method: ContentAddressedMethod,
    hash_algo: HashAlgorithm,
    hash: String
  },
  CAFloating {
    method: ContentAddressedMethod,
    hash_algo: HashAlgorithm
  }
}

#[derive(Debug)]
pub enum InputDrv {
  Paths(HashSet<String>),
  Map(HashMap<String, InputDrv>),
}

impl InputDrv {
  fn parse<'src>(input: &'src str, version: DerivationVersion) -> ParseRes<'src, Self> {
    match version {
      DerivationVersion::Traditional => {
        let (input, strs) = string_set(input)?;
        Ok((input, InputDrv::Paths(strs)))
      },
      DerivationVersion::Dynamic => todo!(),
    }
  }
}

#[derive(Debug, Clone, Copy)]
pub enum DerivationVersion {
  Traditional,
  Dynamic
}

#[derive(Debug)]
pub struct Derivation {
  pub version: DerivationVersion,
  pub name: String,
  pub outputs: HashMap<String, DerivationOutput>,
  pub input_srcs: HashSet<String>,
  pub input_drvs: HashMap<String, InputDrv>,
  pub platform: String,
  pub builder: PathBuf,
  pub args: Vec<String>,
  pub env: HashMap<String, String>
}

impl DerivationOutput {
  fn new(store: &NixStore, path: &str, hash_algo: &str, hash: &str) -> Result<Self> {
    if !hash_algo.is_empty() {
      let (hash, method) = ContentAddressedMethod::parse(hash_algo);
      let Some(hash_algo) = HashAlgorithm::parse(hash) else {
        anyhow::bail!("Unrecognized hash algorithm");
      };
      if hash == "impure" {
        if !path.is_empty() {
          anyhow::bail!("Impure derivation output should not specify output path");
        }
        return Ok(DerivationOutput::Impure { method, hash_algo });
      } else if !hash.is_empty() {
        // TODO: validate path
        return Ok(DerivationOutput::CAFixed { method, hash_algo, hash: hash.to_string()});
      } else {
        if !path.is_empty() {
          anyhow::bail!("Impure derivation output should not specify output path");
        }
        return Ok(DerivationOutput::CAFloating { method, hash_algo });
      }
    } else {
      if path.is_empty() {
        return Ok(DerivationOutput::Deferred);
      }
      return Ok(DerivationOutput::InputAddressed { path: store.parse_path(path)? });
    }
  }
}

fn string_set(input: &str) -> ParseRes<HashSet<String>> {
  list(string)(input).map(|(rest, s)| (rest, s.into_iter().map(|s| s.to_string()).collect()))
}

fn parse_input(input: &str, version: DerivationVersion) -> ParseRes<(String, InputDrv)> {
  let (input, _) = tag("(")(input)?;
  let (input, name) = string(input)?;
  let (input, _) = tag(",")(input)?;
  let (input, input_drv) = InputDrv::parse(input, version)?;
  let (input, _) = tag(")")(input)?;
  Ok((input, (name.to_string(), input_drv)))
}

fn string<'src>(input: &'src str) -> ParseRes<'src, &'src str> {
  let (input, _) = char('"')(input)?;
  let (input, s) = opt(escaped(none_of("\"\\"), '\\', one_of("\"ntr\\")))(input)?;
  let (input, _) = char('"')(input)?;
  Ok((input, s.unwrap_or("")))
}

fn parse_drv_output<'store, 'src>(store: &'store NixStore, input: &'src str) -> ParseRes<'src, (String, DerivationOutput)> {
  let (input, _) = tag("(")(input)?;
  let (input, name) = string(input)?;
  let (input, _) = tag(",")(input)?;
  let (input, path) = string(input)?;
  let (input, _) = tag(",")(input)?;
  let (input, hash_algo) = string(input)?;
  let (input, _) = tag(",")(input)?;
  let (input, hash) = string(input)?;
  let (input, _) = tag(")")(input)?;
  let Ok(drv) = DerivationOutput::new(store, path, hash_algo, hash) else {
    return fail(input);
  };
  Ok((input, (name.to_string(), drv)))
}

fn list<'src, O, P: Parser<&'src str, O, VerboseError<&'src str>>>(parser: P) -> impl FnMut(&'src str) -> ParseRes<'src, Vec<O>> {
  delimited(char('['), separated_list0(char(','), parser), char(']'))
}

fn parse_pair(input: &str) -> ParseRes<(String, String)> {
  let (input, _) = tag("(")(input)?;
  let (input, name) = string(input)?;
  let (input, _) = tag(",")(input)?;
  let (input, value) = string(input)?;
  let (input, _) = tag(")")(input)?;
  Ok((input, (name.to_string(), value.to_string())))
}

fn parse_version(input: &str) -> ParseRes<DerivationVersion> {
  let traditional = value(DerivationVersion::Traditional, tag("Derive("));
  let dynamic = |input| {
    let (input, _) = tag("DrvWithVersion(")(input)?;
    let (input, v) = string(input)?;
    if v != "xp-dyn-drv" {
      return fail(input);
    }
    return Ok((input, DerivationVersion::Dynamic))
  };
  alt((traditional, dynamic))(input)
}

fn parse_derivation<'store, 'src>(store: &'store NixStore, name: String, input: &'src str) -> ParseRes<'src, Derivation> {
  let (input, version) = parse_version(input)?;
  let (input, outputs) = list(|i| parse_drv_output(store, i))(input)?;
  let (input, _) = tag(",")(input)?;
  let (input, input_drvs) = list(move |i| parse_input(i, version))(input)?;
  let (input, _) = tag(",")(input)?;
  let (input, input_srcs) = string_set(input)?;
  let (input, _) = tag(",")(input)?;
  let (input, platform) = string(input)?;
  let (input, _) = tag(",")(input)?;
  let (input, builder) = string(input)?;
  let (input, _) = tag(",")(input)?;
  let (input, args) = list(string)(input)?;
  let (input, _) = tag(",")(input)?;
  let (input, env) = list(parse_pair)(input)?;
  let (input, _) = tag(")")(input)?;
  let drv = Derivation {
    version: DerivationVersion::Traditional,
    name,
    outputs: outputs.into_iter().collect(),
    input_srcs: input_srcs.into_iter().collect(),
    input_drvs: input_drvs.into_iter().collect(),
    platform: platform.to_string(),
    builder: Path::new(builder).to_path_buf(),
    args: args.into_iter().map(str::to_string).collect(),
    env: env.into_iter().collect(),
};
  Ok((input, drv))
}

impl Derivation {
  pub fn from_path(store: &NixStore, drv_path: &NixStorePath) -> Result<Self> {
    let content = std::fs::read_to_string(&drv_path.path)?;
    let drv_name = drv_path.name()?;
    let name = drv_name
      .strip_suffix(".drv")
      .ok_or_else(|| anyhow::format_err!("Path is not derivation."))?;
    let (_, drv) = parse_derivation(store, name.to_string(), &content)
      .finish()
      .map_err(|e| anyhow::format_err!("{}", nom::error::convert_error(content.as_str(), e)))?;
    Ok(drv)
  }
}
