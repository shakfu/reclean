#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use std::fs;
    use toml::Table;

    const PATTERNS: [&str; 14] = [
        // directory
        "**/.coverage",
        "**/.DS_Store",
        // ".egg-info",
        "**/.cache",
        "**/.mypy_cache",
        "**/.pylint_cache",
        "**/.pytest_cache",
        "**/.ruff_cache",
        "**/__pycache__",
        // file
        "**/.bash_history",
        "*.log",
        "*.o",
        "*.py[co]",
        "**/.python_history",
        "**/pip-log.txt",
    ];

    #[derive(Serialize, Deserialize)]
    struct CleaningJob {
        path: String,
        patterns: Vec<String>,
        dry_run: bool,
        skip_confirmation: bool,
        include_symlinks: bool,
        remove_broken_symlinks: bool,
    }

    #[test]
    fn test_toml_basic() {
        let value = "foo = 'bar'".parse::<Table>().unwrap();
        assert_eq!(value["foo"].as_str(), Some("bar"));
    }
    #[test]
    fn test_toml_array() {
        let mut xs: Vec<String> = vec![];
        let value = "foo = ['bar', 'bat']".parse::<Table>().unwrap();
        let array = value["foo"].as_array().unwrap();
        for elem in array {
            xs.push(elem.to_owned().to_string());
        }
        assert!(xs.iter().any(|e| e.contains("bar")));
        assert!(xs.iter().any(|e| e.contains("bat")));
    }

    #[test]
    fn test_toml_load() {
        let contents = fs::read_to_string("tests/.reclean.toml").expect("cannot read file");
        assert!(!contents.is_empty());
    }

    #[test]
    fn test_toml_table_from_file() {
        let contents = fs::read_to_string("tests/.reclean.toml").expect("cannot read file");
        let table = contents.parse::<Table>().unwrap();
        assert_eq!(table["path"].as_str(), Some("."));
        //let array = table["patterns"].as_array();
        assert_eq!(table["dry_run"].as_bool(), Some(false));
    }

    #[test]
    fn test_toml_to_file() {
        let mut job = CleaningJob {
            path: ".".to_string(),
            patterns: vec![],
            dry_run: false,
            skip_confirmation: false,
            include_symlinks: false,
            remove_broken_symlinks: false,
        };
        for p in PATTERNS {
            job.patterns.push(p.to_string());
        }

        let toml = toml::to_string(&job).unwrap();
        assert!(!toml.is_empty());
        let outfile = "tests/out.toml";
        fs::write(outfile, toml).expect("could not write toml to file.");
        assert!(fs::metadata(outfile).unwrap().is_file());
        std::fs::remove_file(outfile).expect("outfile.toml could not be removed");
    }
}
