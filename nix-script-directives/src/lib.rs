#[warn(clippy::cargo)]
pub mod expr;
mod parser;

use crate::expr::Expr;
use anyhow::{Context, Result};
use core::hash::{Hash, Hasher};
use parser::Parser;
use rnix::SyntaxKind;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, serde::Serialize)]
pub struct Directives {
    pub build_command: Option<String>,
    pub build_root: Option<PathBuf>,
    pub build_inputs: Vec<Expr>,
    pub interpreter: Option<String>,
    pub runtime_inputs: Vec<Expr>,
    pub runtime_files: Vec<PathBuf>,
    pub nixpkgs_config: Option<Expr>,
    pub all: HashMap<String, Vec<String>>,
}

impl Directives {
    pub fn from_file(indicator: &str, filename: &Path) -> Result<Self> {
        let source = std::fs::read_to_string(filename).context("could not read source")?;
        Self::parse(indicator, &source)
    }

    fn parse(indicator: &str, source: &str) -> Result<Self> {
        let parser = Parser::new(indicator).context("could not construct parser")?;
        let fields = parser.parse(source);

        Self::from_directives(fields)
    }

    fn from_directives(fields: HashMap<&str, Vec<&str>>) -> Result<Self> {
        let build_command = Self::once("build", &fields)?.map(|s| s.to_owned());
        let build_root = Self::once("buildRoot", &fields)?.map(PathBuf::from);
        let build_inputs = Self::exprs("buildInputs", &fields)?;
        let interpreter = Self::once("interpreter", &fields)?.map(|s| s.to_owned());
        let runtime_inputs = Self::exprs("runtimeInputs", &fields)?;
        let runtime_files = Self::files("runtimeFiles", &fields);
        let nixpkgs_config = Self::once_attrset("nixpkgsConfig", &fields)?;

        Ok(Directives {
            build_command,
            build_root,
            build_inputs,
            interpreter,
            runtime_inputs,
            runtime_files,
            nixpkgs_config,
            all: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
                .collect(),
        })
    }

    fn once<'field>(
        field: &'field str,
        fields: &HashMap<&'field str, Vec<&'field str>>,
    ) -> Result<Option<&'field str>> {
        match fields.get(field) {
            Some(value) => {
                if value.len() != 1 {
                    anyhow::bail!("multiple `{}` directives but need exactly one", field);
                }

                Ok(Some(value[0]))
            }
            None => Ok(None),
        }
    }

    fn once_attrset<'field>(
        field: &'field str,
        fields: &HashMap<&'field str, Vec<&'field str>>,
    ) -> Result<Option<Expr>> {
        match Self::once(field, fields)? {
            Some(raw_options) => {
                let parsed = Expr::from_str(raw_options)
                    .with_context(|| format!("could not parse `{field}` as a Nix expression"))?;

                match parsed.kind() {
                    SyntaxKind::NODE_ATTR_SET => Ok(Some(parsed)),
                    other => anyhow::bail!(
                        "`{}` directive should be a Nix record but is a `{:?}`",
                        field,
                        other
                    ),
                }
            }
            None => Ok(None),
        }
    }

    fn exprs<'field>(
        field: &'field str,
        fields: &HashMap<&'field str, Vec<&'field str>>,
    ) -> Result<Vec<Expr>> {
        match fields.get(field) {
            None => Ok(Vec::new()),
            Some(lines) => {
                Expr::parse_as_list(&lines.join(" ")).context("could not parse runtime inputs")
            }
        }
    }

    fn files<'field>(
        field: &'field str,
        fields: &HashMap<&'field str, Vec<&'field str>>,
    ) -> Vec<PathBuf> {
        match fields.get(field) {
            None => Vec::new(),
            Some(lines) => lines.join(" ").split(' ').map(PathBuf::from).collect(),
        }
    }

    pub fn maybe_override_build_command(&mut self, maybe_new: &Option<String>) {
        if maybe_new.is_some() {
            maybe_new.clone_into(&mut self.build_command)
        }
    }

    pub fn merge_build_inputs(&mut self, new: &[String]) -> Result<()> {
        for item in new {
            let parsed = (item).parse().context("could not parse build input")?;

            if !self.build_inputs.contains(&parsed) {
                self.build_inputs.push(parsed)
            }
        }

        Ok(())
    }

    pub fn override_interpreter(&mut self, interpreter: &str) {
        self.interpreter = Some(interpreter.to_owned());
    }

    pub fn merge_runtime_inputs(&mut self, new: &[String]) -> Result<()> {
        for item in new {
            let parsed = (item).parse().context("could not parse build input")?;

            if !self.runtime_inputs.contains(&parsed) {
                self.runtime_inputs.push(parsed)
            }
        }

        Ok(())
    }

    pub fn merge_runtime_files(&mut self, new: &[PathBuf]) {
        for item in new {
            if !self.runtime_files.contains(item) {
                self.runtime_files.push(item.clone())
            }
        }
    }

    pub fn override_nixpkgs_config(&mut self, expr: &Expr) -> Result<()> {
        match expr.kind() {
            SyntaxKind::NODE_ATTR_SET => self.nixpkgs_config = Some(expr.clone()),
            other => anyhow::bail!(
                "Nixpkgs config was no Nix attribute set, but a `{:?}`",
                other,
            ),
        };

        Ok(())
    }
}

impl Hash for Directives {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        if let Some(build_command) = &self.build_command {
            hasher.write(build_command.as_ref())
        }

        for input in &self.build_inputs {
            input.hash(hasher)
        }

        if let Some(interpreter) = &self.interpreter {
            hasher.write(interpreter.as_ref())
        }

        for input in &self.runtime_inputs {
            input.hash(hasher)
        }

        if let Some(build_root) = &self.build_root {
            hasher.write(build_root.display().to_string().as_ref())
        }

        for file in &self.runtime_files {
            hasher.write(file.display().to_string().as_ref())
        }

        if let Some(nixpkgs_config) = &self.nixpkgs_config {
            hasher.write(nixpkgs_config.to_string().as_ref())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod from_directives {
        use super::*;

        #[test]
        fn only_one_build_command_allowed() {
            let problem = Directives::from_directives(HashMap::from([("build", vec!["a", "b"])]))
                .unwrap_err();

            assert!(problem.to_string().contains("multiple `build` directives"),)
        }

        #[test]
        fn combines_build_inputs() {
            let directives =
                Directives::from_directives(HashMap::from([("buildInputs", vec!["a b", "c d"])]))
                    .unwrap();

            let expected: Vec<Expr> = vec![
                "a".parse().unwrap(),
                "b".parse().unwrap(),
                "c".parse().unwrap(),
                "d".parse().unwrap(),
            ];

            assert_eq!(expected, directives.build_inputs);
        }

        #[test]
        fn only_one_interpreter_allowed() {
            let problem =
                Directives::from_directives(HashMap::from([("interpreter", vec!["a", "b"])]))
                    .unwrap_err();

            assert!(problem
                .to_string()
                .contains("multiple `interpreter` directives"))
        }

        #[test]
        fn combines_runtime_inputs() {
            let directives =
                Directives::from_directives(HashMap::from([("runtimeInputs", vec!["a b", "c d"])]))
                    .unwrap();

            let expected: Vec<Expr> = vec![
                ("a").parse().unwrap(),
                ("b").parse().unwrap(),
                ("c").parse().unwrap(),
                ("d").parse().unwrap(),
            ];

            assert_eq!(expected, directives.runtime_inputs);
        }

        #[test]
        fn only_one_build_root_allowed() {
            let problem =
                Directives::from_directives(HashMap::from([("buildRoot", vec!["a", "b"])]))
                    .unwrap_err();

            assert!(problem
                .to_string()
                .contains("multiple `buildRoot` directives"))
        }

        #[test]
        fn sets_root() {
            let directives =
                Directives::from_directives(HashMap::from([("buildRoot", vec!["."])])).unwrap();

            assert_eq!(Some(PathBuf::from(".")), directives.build_root)
        }

        #[test]
        fn combines_runtime_files() {
            let directives =
                Directives::from_directives(HashMap::from([("runtimeFiles", vec!["a b", "c d"])]))
                    .unwrap();

            let expected = vec![
                PathBuf::from("a"),
                PathBuf::from("b"),
                PathBuf::from("c"),
                PathBuf::from("d"),
            ];

            assert_eq!(expected, directives.runtime_files);
        }

        #[test]
        fn includes_others_raw() {
            let directives =
                Directives::from_directives(HashMap::from([("other", vec!["other"])])).unwrap();

            assert_eq!(
                Some(&vec!["other".to_string()]),
                directives.all.get("other")
            )
        }

        #[test]
        fn only_one_nixpkgs_options_allowed() {
            let problem =
                Directives::from_directives(HashMap::from([("nixpkgsConfig", vec!["{}", "{}"])]))
                    .unwrap_err();

            assert!(problem
                .to_string()
                .contains("multiple `nixpkgsConfig` directives"))
        }

        #[test]
        fn nixpkgs_options_must_be_a_attrset() {
            let problem =
                Directives::from_directives(HashMap::from([("nixpkgsConfig", vec!["1"])]))
                    .unwrap_err();

            assert!(problem.to_string().contains("`nixpkgsConfig` directive"),)
        }

        #[test]
        fn nixpkgs_options_takes_an_attrset() {
            let options = "{ system = \"x86_64-darwin\"; }";
            let directives =
                Directives::from_directives(HashMap::from([("nixpkgsConfig", vec![options])]))
                    .unwrap();

            assert_eq!(
                Some(options.to_string()),
                directives.nixpkgs_config.map(|o| o.to_string()),
            )
        }
    }

    mod hash {
        use super::*;

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn assert_have_different_hashes<H: Hash>(l: H, r: H) {
            let mut l_hasher = DefaultHasher::new();
            let mut r_hasher = DefaultHasher::new();

            l.hash(&mut l_hasher);
            r.hash(&mut r_hasher);

            println!("l: {}, r: {}", l_hasher.finish(), r_hasher.finish());
            assert!(l_hasher.finish() != r_hasher.finish())
        }

        #[test]
        fn build_command_changes_hash() {
            assert_have_different_hashes(
                Directives::from_directives(HashMap::from([("build", vec!["a"])])).unwrap(),
                Directives::from_directives(HashMap::from([("build", vec!["b"])])).unwrap(),
            )
        }

        #[test]
        fn build_inputs_changes_hash() {
            assert_have_different_hashes(
                Directives::from_directives(HashMap::from([("buildInputs", vec!["a"])])).unwrap(),
                Directives::from_directives(HashMap::from([("buildInputs", vec!["b"])])).unwrap(),
            )
        }

        #[test]
        fn interpreter_changes_hash() {
            assert_have_different_hashes(
                Directives::from_directives(HashMap::from([("interpreter", vec!["a"])])).unwrap(),
                Directives::from_directives(HashMap::from([("interpreter", vec!["b"])])).unwrap(),
            )
        }

        #[test]
        fn runtime_inputs_changes_hash() {
            assert_have_different_hashes(
                Directives::from_directives(HashMap::from([("runtimeInputs", vec!["a"])])).unwrap(),
                Directives::from_directives(HashMap::from([("runtimeInputs", vec!["b"])])).unwrap(),
            )
        }

        #[test]
        fn root_changes_hash() {
            assert_have_different_hashes(
                Directives::from_directives(HashMap::from([("buildRoot", vec!["a"])])).unwrap(),
                Directives::from_directives(HashMap::from([("buildRoot", vec!["b"])])).unwrap(),
            )
        }

        #[test]
        fn runtime_files_change_hash() {
            assert_have_different_hashes(
                Directives::from_directives(HashMap::from([("runtimeFiles", vec!["a"])])).unwrap(),
                Directives::from_directives(HashMap::from([("runtimeFiles", vec!["b"])])).unwrap(),
            )
        }

        #[test]
        fn nixpkgs_config_changes_hash() {
            assert_have_different_hashes(
                Directives::from_directives(HashMap::from([(
                    "nixpkgsConfig",
                    vec!["{ system = \"x86_64-darwin\"; }"],
                )]))
                .unwrap(),
                Directives::from_directives(HashMap::from([(
                    "nixpkgsConfig",
                    vec!["{ system = \"aarch64-darwin\"; }"],
                )]))
                .unwrap(),
            )
        }
    }
}
