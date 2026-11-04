use perveance::file_browser::{chosen_file, list_directory, EntryKind, MAX_DIRECTORY_ENTRIES};
use std::fs;
use std::path::Path;

#[test]
fn listing_sorts_directories_and_filters_hidden_names() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("z-directory")).unwrap();
    fs::write(dir.path().join("a-file"), b"original").unwrap();
    fs::write(dir.path().join(".secret"), b"hidden").unwrap();
    let list = list_directory(dir.path(), false).unwrap();
    assert_eq!(list.entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["z-directory", "a-file"]);
    assert_eq!(list.entries[0].kind, EntryKind::Directory);
    assert_eq!(list.entries[1].kind, EntryKind::File);
    assert!(list_directory(dir.path(), true).unwrap().entries.iter().any(|e| e.name == ".secret"));
    assert_eq!(fs::read(dir.path().join("a-file")).unwrap(), b"original");
}

#[test]
fn selecting_a_filename_does_not_write_and_rejects_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let path = chosen_file(dir.path(), "new.perveance.json").unwrap();
    assert_eq!(path, dir.path().join("new.perveance.json"));
    assert!(!path.exists());
    for name in ["", ".", "..", "../other", "nested/file", "/absolute"] {
        assert!(chosen_file(dir.path(), name).is_err(), "{name:?}");
    }
    assert!(list_directory(Path::new("/a/missing/perveance/directory"), false).is_err());
}

#[test]
fn huge_directories_return_an_error_instead_of_a_partial_result() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..=MAX_DIRECTORY_ENTRIES { fs::write(dir.path().join(format!(".entry-{i}")), b"").unwrap(); }
    assert!(list_directory(dir.path(), false).unwrap_err().contains("exceeds"));
    assert!(list_directory(dir.path(), true).unwrap_err().contains("exceeds"));
}

#[cfg(unix)]
#[test]
fn links_are_visible_but_not_followed_and_non_utf8_names_are_reported() {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    symlink(dir.path().join("missing"), dir.path().join("dangling")).unwrap();
    fs::write(dir.path().join(std::ffi::OsString::from_vec(vec![0xff])), b"").unwrap();
    let list = list_directory(dir.path(), true).unwrap();
    assert_eq!(list.skipped, 1);
    assert_eq!(list.entries.len(), 1);
    assert_eq!(list.entries[0].kind, EntryKind::Symlink);
    assert_eq!(list.entries[0].name, "dangling");
}
