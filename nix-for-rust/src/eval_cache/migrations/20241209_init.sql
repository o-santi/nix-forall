CREATE TABLE IF NOT EXISTS evaluation_output (
  id             INTEGER NOT NULL PRIMARY KEY,
  main_file_path BLOB NOT NULL,
  accessor_path  TEXT NOT NULL,
  output         TEXT NOT NULL,
  main_file_hash CHAR(64) NOT NULL, 
  input_hash     CHAR(64) NOT NULL,
  UNIQUE(input_hash, accessor_path)
);

CREATE INDEX IF NOT EXISTS idx_evaluation_output_accessor_path_file_path_file_hash ON evaluation_output(accessor_path, main_file_path, main_file_hash);

CREATE TABLE IF NOT EXISTS evaluation_input (
  evaluation_id  INTEGER NOT NULL,
  file_path      BLOB NOT NULL,
  FOREIGN KEY (evaluation_id) REFERENCES evaluation_output(id)
    ON UPDATE CASCADE ON DELETE CASCADE 
);

CREATE INDEX IF NOT EXISTS idx_evaluation_input_evaluation_id ON evaluation_input(evaluation_id);
