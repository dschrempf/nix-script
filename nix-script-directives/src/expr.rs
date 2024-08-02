use anyhow::{Context, Result};
use core::hash::{Hash, Hasher};
use rnix::ast::List;
use rnix::{Root, SyntaxKind, SyntaxNode};
use rowan::ast::AstNode;
use std::cmp::Ordering;
use std::fmt::{self, Display};
use std::str::FromStr;

#[derive(Debug, Eq, serde::Serialize, Clone)]
pub struct Expr {
    raw: String,
    #[serde(skip)]
    parsed: SyntaxNode,
}

impl Expr {
    pub fn parse_as_list(source: &str) -> Result<Vec<Self>> {
        let root: Root = Root::parse(&format!("[{source}]"))
            .ok()
            .context("could not parse Nix expression as list")?;
        let syntax_node: SyntaxNode = root
            .expr()
            .expect("root of ast should have a child")
            .syntax()
            .clone();
        let list: List =
            List::cast(syntax_node).context("could not get back expression as list")?;

        Ok(list
            .items()
            .map(|e| Self::from(e.syntax().clone()))
            .collect())
    }

    pub fn kind(&self) -> SyntaxKind {
        self.parsed.kind()
    }

    pub fn is_leaf(&self) -> bool {
        self.kind() == SyntaxKind::NODE_IDENT
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.raw)
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl Ord for Expr {
    fn cmp(&self, other: &Self) -> Ordering {
        self.raw.cmp(&other.raw)
    }
}

impl PartialOrd for Expr {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for Expr {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        hasher.write(self.raw.as_ref())
    }
}

impl FromStr for Expr {
    type Err = anyhow::Error;

    fn from_str(source: &str) -> Result<Self> {
        Ok(Self::from(
            Root::parse(source)
                .ok()
                .context("could not parse Nix expression")?
                .expr()
                .expect("root of AST should not be empty")
                .syntax()
                .clone(),
        ))
    }
}

/// Unwrap parentheses.
impl From<SyntaxNode> for Expr {
    fn from(outer: SyntaxNode) -> Expr {
        if outer.kind() == SyntaxKind::NODE_PAREN {
            if let Some(inner) = outer.children().next() {
                return Self::from(inner);
            }
        }

        Self {
            raw: outer.to_string(),
            parsed: outer,
        }
    }
}

unsafe impl Send for Expr {}

unsafe impl Sync for Expr {}

#[cfg(test)]
mod tests {
    use super::*;

    mod equality {
        use super::*;

        #[test]
        fn equal_if_raw_is_equal() {
            assert_eq!(Expr::from_str("a").unwrap(), Expr::from_str("a").unwrap())
        }

        #[test]
        fn unequal_if_raw_is_unequal() {
            assert!(Expr::from_str("a").unwrap() != Expr::from_str("b").unwrap())
        }
    }

    mod parse {
        use super::*;

        #[test]
        fn accepts_valid() {
            assert!(Expr::from_str("a").is_ok())
        }

        #[test]
        fn rejects_invalid() {
            assert!(Expr::from_str("[").is_err())
        }

        #[test]
        fn unwraps_root() {
            assert_eq!(SyntaxKind::NODE_IDENT, Expr::from_str("a").unwrap().kind())
        }

        #[test]
        fn unwraps_parens() {
            assert_eq!(
                SyntaxKind::NODE_IDENT,
                Expr::from_str("(a)").unwrap().kind()
            )
        }

        #[test]
        fn unwraps_all_parens() {
            assert_eq!(
                SyntaxKind::NODE_IDENT,
                Expr::from_str("((a))").unwrap().kind()
            )
        }
    }

    mod parse_as_list {
        use super::*;

        #[test]
        fn single_item() {
            let parsed = Expr::parse_as_list("a").unwrap();

            assert_eq!(1, parsed.len());
            assert_eq!("a", parsed[0].raw);
        }

        #[test]
        fn multiple_items() {
            let parsed = Expr::parse_as_list("a b").unwrap();

            assert_eq!(2, parsed.len());
            assert_eq!("a", parsed[0].raw);
            assert_eq!("b", parsed[1].raw);
        }
    }

    mod is_leaf {
        use super::*;

        #[test]
        fn ident_yes() {
            let parsed = Expr::from_str("a").unwrap();
            assert!(parsed.is_leaf());
        }

        #[test]
        fn apply_no() {
            let parsed =
                Expr::from_str("haskellPackages.ghcWithPackages (ps: [ ps.text ])").unwrap();
            assert!(!parsed.is_leaf());
        }
    }

    mod display {
        use super::*;

        #[test]
        fn same_as_node() {
            let parsed = Expr::from_str("a b c").unwrap();
            assert_eq!(parsed.to_string(), parsed.parsed.to_string());
        }
    }
}
