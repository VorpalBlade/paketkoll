use camino::Utf8Path;
use camino::Utf8PathBuf;

/// Safe path join that does not replace when the second path is absolute
#[must_use]
pub fn safe_path_join(left: &Utf8Path, right: &Utf8Path) -> Utf8PathBuf {
    let right = if right.is_absolute() {
        right
            .strip_prefix("/")
            .expect("We know the path is absolute")
    } else {
        right
    };
    left.join(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_path_join() {
        assert_eq!(
            safe_path_join(Utf8Path::new("/a/b"), Utf8Path::new("c/d")),
            Utf8PathBuf::from("/a/b/c/d")
        );
        assert_eq!(
            safe_path_join(Utf8Path::new("/a/b"), Utf8Path::new("/c/d")),
            Utf8PathBuf::from("/a/b/c/d")
        );
    }
}
