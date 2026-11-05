use super::*;
use std::{fs, os::unix::{fs::symlink, net::UnixListener}, sync::Arc, thread};

fn private() -> tempfile::TempDir {
    tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap()
}
fn write_private(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn artwork() -> Project {
    let mut project = Project::new(2, 3).unwrap();
    project.rename_layer(0, "private-layer-name").unwrap();
    project.layers[0].visible = false;
    project.set_pixel(0, 1, 1, [7, 8, 9, 0]).unwrap();
    project.add_layer().unwrap();
    project.set_pixel(1, 0, 0, [3, 2, 1, 128]).unwrap();
    project
}

#[test]
fn path_grammar_rejects_traversal_ambiguity_and_oversized_names() {
    for name in ["", ".", "..", "../x", "a/../x", "a/./x", "a//b", "/x", "a/", "a\nb", "a\0b", "C:\\x", "a\\b"] {
        assert!(relative(name).is_err(), "{name:?}");
    }
    assert!(relative(&"x".repeat(256)).is_err());
    assert!(relative(&["a"; 9].join("/")).is_err());
    assert!(relative(&vec!["x".repeat(200); 8].join("/")).is_err());
    assert!(relative("sub/art.json").is_ok());
    for name in ["", ".", "..", ".private", "../x", "a/b", "a\\b", "a\nb", "x y", "art:1"] {
        assert!(bundle_name(name).is_err());
    }
    assert!(bundle_name("result-1.2_3").is_ok());
}

#[test]
fn source_types_links_privacy_and_byte_limits_are_checked_before_read() {
    let input = private();
    let root = PrivateDirectory::open(input.path()).unwrap();
    let p = input.path();
    write_private(&p.join("original"), b"private-sentinel");
    fs::hard_link(p.join("original"), p.join("hardlink")).unwrap();
    assert!(root.read("hardlink", 100).is_err());
    fs::remove_file(p.join("hardlink")).unwrap();
    symlink("original", p.join("link")).unwrap();
    fs::create_dir(p.join("directory")).unwrap();
    let fifo = CString::new(p.join("fifo").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let _socket = UnixListener::bind(p.join("socket")).unwrap();
    for name in ["link", "directory", "fifo", "socket"] {
        let reached_read = std::cell::Cell::new(false);
        assert!(root.read_with_hook(name, 100, || reached_read.set(true)).is_err());
        assert!(!reached_read.get(), "special file must be rejected before readable open");
    }
    fs::set_permissions(p.join("original"), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(root.read("original", 100).is_err());
    fs::set_permissions(p.join("original"), fs::Permissions::from_mode(0o600)).unwrap();
    assert!(root.read("original", 3).is_err());
    assert_eq!(root.read("original", 100).unwrap(), b"private-sentinel");
}

#[test]
fn final_and_intermediate_symlinks_and_magic_links_never_escape() {
    let input = private();
    let outside = private();
    write_private(&outside.path().join("secret"), b"outside-sentinel");
    symlink(outside.path(), input.path().join("escape")).unwrap();
    symlink("/proc/self/fd", input.path().join("magic")).unwrap();
    let root = PrivateDirectory::open(input.path()).unwrap();
    assert!(root.read("escape/secret", 100).is_err());
    assert!(root.read("magic/0", 100).is_err());
    let alias = outside.path().join("alias");
    symlink(input.path(), &alias).unwrap();
    assert!(PrivateDirectory::open(&alias).is_err());
    fs::create_dir(input.path().join("child")).unwrap();
    assert!(PrivateDirectory::open(&alias.join("child")).is_err());
    assert_eq!(fs::read(outside.path().join("secret")).unwrap(), b"outside-sentinel");
}

#[test]
fn held_inode_capture_does_not_follow_a_replaced_leaf_or_accept_live_mutation() {
    let input = private();
    let outside = private();
    write_private(&input.path().join("source"), b"original-data");
    write_private(&outside.path().join("secret"), b"outside-sentinel");
    let root = PrivateDirectory::open(input.path()).unwrap();
    let result = root.read_with_hook("source", 100, || {
        fs::rename(input.path().join("source"), input.path().join("kept")).unwrap();
        symlink(outside.path().join("secret"), input.path().join("source")).unwrap();
    });
    // Renaming may change ctime and fail the stability check; neither outcome
    // can consume the symlink target instead of the held inode.
    if let Ok(bytes) = result { assert_eq!(bytes, b"original-data"); }
    assert!(root.read_with_hook("kept", 100, || write_private(&input.path().join("kept"), b"changed-size")).is_err());
    assert_eq!(fs::read(outside.path().join("secret")).unwrap(), b"outside-sentinel");
}

#[test]
fn snapshot_seals_reject_write_truncate_grow_and_offset_sharing() {
    let snapshot = ProjectSnapshot::from_project(&artwork()).unwrap();
    assert!(has_required_seals(&snapshot.frozen.file));
    assert!(snapshot.frozen.file.write_at(b"x", 0).is_err());
    assert!(snapshot.frozen.file.set_len(0).is_err());
    assert!(snapshot.frozen.file.set_len(snapshot.frozen.artifact.bytes + 1).is_err());
    let mut first = snapshot.stdin_file().unwrap();
    let mut prefix = [0; 7];
    first.read_exact(&mut prefix).unwrap();
    let mut second = snapshot.stdin_file().unwrap();
    let mut bytes = Vec::new();
    second.read_to_end(&mut bytes).unwrap();
    assert_eq!(&prefix, &bytes[..7]);
    assert_eq!(serde_json::from_slice::<Project>(&bytes).unwrap(), artwork());
}

#[test]
fn root_capabilities_survive_path_rename_and_replacement_without_redirection() {
    let parent = private();
    let original = parent.path().join("inbox");
    fs::create_dir(&original).unwrap();
    fs::set_permissions(&original, fs::Permissions::from_mode(0o700)).unwrap();
    artwork().save_new(&original.join("project")).unwrap();
    let root = PrivateDirectory::open(&original).unwrap();
    let moved = parent.path().join("moved");
    fs::rename(&original, &moved).unwrap();
    let outside = private();
    symlink(outside.path(), &original).unwrap();
    let snapshot = ProjectSnapshot::capture(&root, "project").unwrap();
    assert_eq!(snapshot.project(), &artwork());
    let report = VerifiedBundle::render(snapshot.project()).unwrap().publish(&root, "result").unwrap();
    assert!(report.committed && moved.join("result").is_dir());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn candidate_validation_rejects_project_and_png_mismatches_without_publication() {
    let candidate = private();
    let root = PrivateDirectory::open(candidate.path()).unwrap();
    let expected = artwork();
    expected.save_new(&candidate.path().join(PROJECT_FILE)).unwrap();
    Project::new(1, 1).unwrap().export_png_new(&candidate.path().join(PNG_FILE)).unwrap();
    assert!(VerifiedBundle::capture(&root, &expected).is_err());
    fs::remove_file(candidate.path().join(PNG_FILE)).unwrap();
    expected.export_png_new(&candidate.path().join(PNG_FILE)).unwrap();
    assert!(VerifiedBundle::capture(&root, &Project::new(2, 3).unwrap()).is_err());
    assert!(VerifiedBundle::capture(&root, &expected).is_ok());
    let oversized = File::create(candidate.path().join(PNG_FILE)).unwrap();
    oversized.set_len(MAX_PNG_BYTES + 1).unwrap();
    assert!(VerifiedBundle::capture(&root, &expected).is_err());
}

#[test]
fn canonical_publication_strips_untrusted_png_metadata_and_never_rereads_candidates() {
    let candidate = private();
    let output = private();
    let expected = artwork();
    expected.save_new(&candidate.path().join(PROJECT_FILE)).unwrap();
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, expected.width, expected.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.add_text_chunk("Comment".to_owned(), "candidate-metadata-sentinel".to_owned()).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&expected.rgba().unwrap()).unwrap();
        writer.finish().unwrap();
    }
    write_private(&candidate.path().join(PNG_FILE), &png);
    let verified = VerifiedBundle::capture(&PrivateDirectory::open(candidate.path()).unwrap(), &expected).unwrap();
    write_private(&candidate.path().join(PROJECT_FILE), b"not JSON any more");
    write_private(&candidate.path().join(PNG_FILE), b"not a PNG any more");
    let report = verified.publish(&PrivateDirectory::open(output.path()).unwrap(), "result").unwrap();
    assert!(report.committed && report.parent_directory_synced && !report.autonomous_execution);
    let published = fs::read(output.path().join("result").join(PNG_FILE)).unwrap();
    assert_eq!(published, expected.png_bytes(1).unwrap());
    assert!(!published.windows(b"candidate-metadata-sentinel".len()).any(|w| w == b"candidate-metadata-sentinel"));
    assert_eq!(Project::load(&output.path().join("result").join(PROJECT_FILE)).unwrap(), expected);
}

#[test]
fn partial_and_precommit_failures_leave_no_visible_or_partial_bundle() {
    let output = private();
    let root = PrivateDirectory::open(output.path()).unwrap();
    let verified = VerifiedBundle::render(&artwork()).unwrap();
    for failure in [Phase::FileSynced(0), Phase::FileSynced(1), Phase::BeforeCommit] {
        assert!(verified.publish_with_hook(&root, "result", |phase| {
            assert!(!output.path().join("result").exists());
            if phase == failure { Err(()) } else { Ok(()) }
        }).is_err());
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
    }
}

#[test]
fn a_postcommit_sync_failure_reports_installed_artwork_instead_of_error_or_deletion() {
    let output = private();
    let root = PrivateDirectory::open(output.path()).unwrap();
    let verified = VerifiedBundle::render(&artwork()).unwrap();
    let report = verified.publish_with_hook(&root, "result", |phase| {
        if phase == Phase::Committed { Err(()) } else { Ok(()) }
    }).unwrap();
    assert!(report.committed && !report.parent_directory_synced);
    assert!(output.path().join("result").join(PROJECT_FILE).exists());
    assert!(verified.publish(&root, "result").is_err());
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 1);
}

#[test]
fn concurrent_publication_has_one_winner_and_does_not_share_snapshot_offsets() {
    let output = private();
    let root = Arc::new(PrivateDirectory::open(output.path()).unwrap());
    let bundle = Arc::new(VerifiedBundle::render(&artwork()).unwrap());
    let threads: Vec<_> = (0..4).map(|_| {
        let root = Arc::clone(&root);
        let bundle = Arc::clone(&bundle);
        thread::spawn(move || bundle.publish(&root, "same-name"))
    }).collect();
    assert_eq!(threads.into_iter().map(|worker| worker.join().unwrap().is_ok()).filter(|ok| *ok).count(), 1);
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 1);
    assert_eq!(Project::load(&output.path().join("same-name").join(PROJECT_FILE)).unwrap(), artwork());
}

#[test]
fn kernel_mount_crossing_and_magic_link_denials_are_not_pathname_simulations() {
    let slash = File::open("/").unwrap();
    let proc_name = CString::new("proc").unwrap();
    let control = open_at(slash.as_raw_fd(), &proc_name, libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
        0, RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS).unwrap();
    assert_ne!(slash.metadata().unwrap().dev(), control.metadata().unwrap().dev(), "real proc mount required");
    assert!(open_at(slash.as_raw_fd(), &proc_name, libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC, 0, BENEATH).is_err());
    let fds = File::open(format!("/proc/{}/fd", std::process::id())).unwrap();
    let held_number = CString::new(slash.as_raw_fd().to_string()).unwrap();
    assert!(open_at(fds.as_raw_fd(), &held_number, libc::O_RDONLY | libc::O_CLOEXEC, 0, RESOLVE_NO_MAGICLINKS).is_err());
}

#[test]
fn single_file_precommit_failures_cleanup_and_postcommit_uncertainty_retains_output() {
    let output=private(); let root=PrivateDirectory::open(output.path()).unwrap();
    for failure in [Phase::FileSynced(0), Phase::BeforeCommit] {
        assert!(root.publish_bytes_with_hook("document.json", b"private-doc", 1024, |phase| {
            assert!(!output.path().join("document.json").exists());
            if phase == failure { Err(()) } else { Ok(()) }
        }).is_err());
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
    }
    let result=root.publish_bytes_with_hook("document.json", b"private-doc", 1024, |phase| {
        if phase == Phase::Committed { Err(()) } else { Ok(()) }
    }).unwrap();
    assert!(result.committed && !result.parent_directory_synced);
    assert_eq!(fs::read(output.path().join("document.json")).unwrap(), b"private-doc");
    assert!(root.publish_bytes("document.json", b"replacement", 1024).is_err());
    assert!(root.publish_bytes("other.json", b"too large", 1).is_err());
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 1);
}

#[test]
fn concurrent_single_file_publication_has_exactly_one_winner() {
    let output=private(); let root=Arc::new(PrivateDirectory::open(output.path()).unwrap());
    let threads: Vec<_>=(0..4).map(|i| {
        let root=Arc::clone(&root); thread::spawn(move || root.publish_bytes("same.json", &[i;16], 16))
    }).collect();
    assert_eq!(threads.into_iter().map(|t| t.join().unwrap().is_ok()).filter(|ok| *ok).count(), 1);
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 1);
    let bytes=fs::read(output.path().join("same.json")).unwrap(); assert_eq!(bytes.len(),16);
    assert!(bytes.iter().all(|b| *b==bytes[0]));
}

#[test]
fn replaced_single_file_stage_is_neither_published_nor_deleted_as_our_artwork() {
    let output=private(); let root=PrivateDirectory::open(output.path()).unwrap();
    assert!(root.publish_bytes_with_hook("document.json", b"our bytes", 1024, |phase| {
        if phase == Phase::BeforeCommit {
            let stage=fs::read_dir(output.path()).unwrap().next().unwrap().unwrap().path();
            fs::remove_file(&stage).unwrap(); write_private(&stage, b"not ours");
        }
        Ok(())
    }).is_err());
    assert!(!output.path().join("document.json").exists());
    let remaining=fs::read_dir(output.path()).unwrap().next().unwrap().unwrap().path();
    assert_eq!(fs::read(remaining).unwrap(), b"not ours");
}

#[test]
fn fixed_member_directory_publication_has_atomic_failure_and_uncertainty_semantics() {
    let output=private(); let root=PrivateDirectory::open(output.path()).unwrap();
    let files:[(&'static str,&[u8]);3]=[("manifest.json",b"manifest"),("artifact.wasm",b"opaque artifact"),("receipt.json",b"capture only")];
    for failure in [Phase::FileSynced(0),Phase::FileSynced(1),Phase::FileSynced(2),Phase::BeforeCommit] {
        assert!(root.publish_files_with_hook("package",&files,1024,|phase| {
            assert!(!output.path().join("package").exists()); if phase==failure {Err(())} else {Ok(())}
        }).is_err());
        assert_eq!(fs::read_dir(output.path()).unwrap().count(),0);
    }
    assert!(root.publish_files("package",&[("../escape",b"x")],1024).is_err());
    assert!(root.publish_files("package",&[("same",b"x"),("same",b"y")],1024).is_err());
    assert!(root.publish_files("package",&files,1).is_err()); assert_eq!(fs::read_dir(output.path()).unwrap().count(),0);
    let report=root.publish_files_with_hook("package",&files,1024,|phase| if phase==Phase::Committed {Err(())} else {Ok(())}).unwrap();
    assert!(report.committed && !report.parent_directory_synced); assert_eq!(report.artifacts.len(),3);
    assert_eq!(fs::read_dir(output.path().join("package")).unwrap().count(),3);
    assert!(root.publish_files("package",&files,1024).is_err()); assert_eq!(fs::read_dir(output.path()).unwrap().count(),1);
}

#[test]
fn bounded_directory_enumeration_is_independent_descriptor_based_and_never_activates_entries() {
    let output=private(); let root=Arc::new(PrivateDirectory::open(output.path()).unwrap());
    write_private(&output.path().join("file"),b"private data"); symlink("/etc/passwd",output.path().join("link")).unwrap();
    assert_eq!(root.list_names(2).unwrap(),vec!["file","link"]); assert!(root.list_names(1).is_err());
    let threads:Vec<_>=(0..4).map(|_| {let root=Arc::clone(&root);thread::spawn(move ||root.list_names(2).unwrap())}).collect();
    for thread in threads {assert_eq!(thread.join().unwrap(),vec!["file","link"]);}
    let parent=private(); let moved=parent.path().join("held"); fs::rename(output.path(),&moved).unwrap();
    fs::create_dir(output.path()).unwrap(); fs::set_permissions(output.path(),fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(root.list_names(2).unwrap(),vec!["file","link"]);
    root.publish_files("new-package",&[("member",b"held directory only")],1024).unwrap();
    assert!(moved.join("new-package").exists()); assert!(!output.path().join("new-package").exists());
}

#[test]
fn replaced_directory_stage_is_not_published_and_other_host_data_is_retained() {
    let output=private(); let root=PrivateDirectory::open(output.path()).unwrap();
    assert!(root.publish_files_with_hook("package",&[("member",b"ours")],1024,|phase| {
        if phase==Phase::BeforeCommit {
            let stage=fs::read_dir(output.path()).unwrap().next().unwrap().unwrap().path();
            fs::rename(&stage,output.path().join("host-moved-ours")).unwrap();
            fs::create_dir(&stage).unwrap(); fs::set_permissions(&stage,fs::Permissions::from_mode(0o700)).unwrap();
            write_private(&stage.join("member"),b"not ours");
        }
        Ok(())
    }).is_err());
    assert!(!output.path().join("package").exists());
    let decoy=fs::read_dir(output.path()).unwrap().map(|entry|entry.unwrap().path()).find(|path|path.file_name().unwrap().to_string_lossy().starts_with(STAGE_PREFIX)).unwrap();
    assert_eq!(fs::read(decoy.join("member")).unwrap(),b"not ours");
}
