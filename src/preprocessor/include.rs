//! Header lookup for `#include`.
//!
//! The resolver searches three places in this order:
//!
//! 1. The directory of the file that holds the directive. This applies to
//!    `#include "name"` only.
//! 2. Each `-I` directory, in command-line order.
//! 3. The bundled headers.

use std::path::{Path, PathBuf};

/// Headers compiled into the binary, so an install needs no data files.
static BUNDLED: &[(&str, &str)] = &[
    ("limits.h", include_str!("../headers/limits.h")),
    ("stddef.h", include_str!("../headers/stddef.h")),
    ("stdbool.h", include_str!("../headers/stdbool.h")),
    ("stdint.h", include_str!("../headers/stdint.h")),
    ("stdio.h", include_str!("../headers/stdio.h")),
    ("string.h", include_str!("../headers/string.h")),
    ("stdlib.h", include_str!("../headers/stdlib.h")),
];

#[derive(Debug)]
pub enum Resolved {
    Bundled(&'static str),
    File(PathBuf),
}

pub struct IncludeResolver {
    search: Vec<PathBuf>,
}

impl IncludeResolver {
    pub fn new(search: Vec<PathBuf>) -> IncludeResolver {
        IncludeResolver { search }
    }

    pub fn resolve(&self, name: &str, angled: bool, from_dir: Option<&Path>) -> Option<Resolved> {
        if !angled
            && let Some(dir) = from_dir
        {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(Resolved::File(candidate));
            }
        }
        for dir in &self.search {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(Resolved::File(candidate));
            }
        }
        BUNDLED
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, text)| Resolved::Bundled(text))
    }

    /// The search list, for the "file not found" diagnostic.
    pub fn searched_description(&self) -> String {
        let mut out = String::new();
        for dir in &self.search {
            out.push_str(&format!("{}, ", dir.display()));
        }
        out.push_str("<bundled headers>");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_include_finds_a_bundled_header() {
        let r = IncludeResolver::new(Vec::new());
        assert!(matches!(r.resolve("limits.h", true, None), Some(Resolved::Bundled(_))));
    }

    #[test]
    fn quoted_include_also_falls_back_to_the_bundled_set() {
        // C leaves this to the implementation. Falling back keeps
        // `#include "stdio.h"` working, which real programs do write.
        let r = IncludeResolver::new(Vec::new());
        assert!(matches!(r.resolve("stdio.h", false, None), Some(Resolved::Bundled(_))));
    }

    #[test]
    fn an_unknown_header_does_not_resolve() {
        let r = IncludeResolver::new(Vec::new());
        assert!(r.resolve("nope.h", true, None).is_none());
    }

    #[test]
    fn every_bundled_header_carries_an_include_guard() {
        for (name, text) in BUNDLED {
            assert!(text.contains("#ifndef _VBRCC_"), "{name} has no guard");
            assert!(text.contains("#endif"), "{name} has no #endif");
        }
    }

    #[test]
    fn search_paths_are_listed_for_the_error_message() {
        let r = IncludeResolver::new(vec![PathBuf::from("/tmp/inc")]);
        let text = r.searched_description();
        assert!(text.contains("tmp") && text.contains("inc"), "got: {text}");
        assert!(text.contains("<bundled headers>"), "got: {text}");
    }

    #[test]
    fn a_search_directory_shadows_a_bundled_header() {
        let dir = std::env::temp_dir().join("vbrcc_inc_shadow");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("limits.h"), "/* mine */\n").unwrap();
        let r = IncludeResolver::new(vec![dir]);
        assert!(matches!(r.resolve("limits.h", true, None), Some(Resolved::File(_))));
    }
}
