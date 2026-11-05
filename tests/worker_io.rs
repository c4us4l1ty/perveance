#[cfg(target_os = "linux")]
mod linux {
    use perveance::{worker_io::{PrivateDirectory, ProjectSnapshot, VerifiedBundle, PROJECT_FILE, PNG_FILE}, Project};
    use std::{fs, os::unix::fs::{symlink, PermissionsExt}, path::Path, process::Command};

    fn private() -> tempfile::TempDir {
        tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap()
    }
    fn cli(args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_perveance-cli")).args(args).output().unwrap()
    }
    fn art() -> Project {
        let mut project = Project::new(4, 5).unwrap();
        project.rename_layer(0, "do-not-echo-private-name").unwrap();
        project.set_pixel(0, 1, 2, [11, 22, 33, 0]).unwrap();
        project.layers[0].visible = false;
        project.add_layer().unwrap();
        project.set_pixel(1, 0, 0, [44, 55, 66, 128]).unwrap();
        project
    }
    fn as_str(path: &Path) -> &str { path.to_str().unwrap() }

    #[test]
    fn snapshot_is_full_and_independent_of_original_files_after_capture() {
        let input = private();
        let project = art();
        project.save_new(&input.path().join("source.json")).unwrap();
        let root = PrivateDirectory::open(input.path()).unwrap();
        let snapshot = ProjectSnapshot::capture(&root, "source.json").unwrap();
        fs::remove_file(input.path().join("source.json")).unwrap();
        Project::new(1, 1).unwrap().save_new(&input.path().join("source.json")).unwrap();
        assert_eq!(snapshot.project(), &project);
        assert_eq!(snapshot.revision(), project.revision().unwrap());
        assert_eq!(snapshot.project().sample_layer(0, 1, 2).unwrap(), [11, 22, 33, 0]);
        assert!(!snapshot.project().layers[0].visible);
    }

    #[test]
    fn oversized_sparse_project_and_bad_json_are_rejected_before_publication() {
        let input = private();
        let output = private();
        let source = input.path().join("bad.json");
        fs::write(&source, b"private-sentinel-not-json").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        let result = cli(&["publish-raster-bundle", as_str(input.path()), "bad.json", as_str(output.path()), "result"]);
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("private-sentinel-not-json"));
        fs::OpenOptions::new().write(true).open(&source).unwrap().set_len(40 * 1024 * 1024 + 1).unwrap();
        let root = PrivateDirectory::open(input.path()).unwrap();
        assert!(ProjectSnapshot::capture(&root, "bad.json").is_err());
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
    }

    #[test]
    fn bundle_cli_publishes_verified_editable_project_and_png_once() {
        let input = private();
        let output = private();
        let project = art();
        let path = input.path().join("project.json");
        project.save_new(&path).unwrap();
        let before = fs::read(&path).unwrap();
        let args = ["publish-raster-bundle", as_str(input.path()), "project.json", as_str(output.path()), "artwork-01"];
        let result = cli(&args);
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["ok"], true);
        assert_eq!(report["publication"]["committed"], true);
        assert_eq!(report["publication"]["parent_directory_synced"], true);
        assert_eq!(report["publication"]["autonomous_execution"], false);
        assert_eq!(report["publication"]["revision"], project.revision().unwrap());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("do-not-echo-private-name"));
        let bundle = output.path().join("artwork-01");
        assert_eq!(fs::read_dir(&bundle).unwrap().count(), 2);
        assert_eq!(Project::load(&bundle.join(PROJECT_FILE)).unwrap(), project);
        let imported = Project::from_png(&bundle.join(PNG_FILE)).unwrap();
        assert_eq!(imported.rgba().unwrap(), project.rgba().unwrap());
        assert_eq!(fs::metadata(&bundle).unwrap().permissions().mode() & 0o777, 0o700);
        for artifact in report["publication"]["artifacts"].as_array().unwrap() {
            use sha2::{Digest, Sha256};
            let bytes = fs::read(bundle.join(artifact["file"].as_str().unwrap())).unwrap();
            assert_eq!(artifact["bytes"], bytes.len() as u64);
            assert_eq!(artifact["sha256"], format!("{:x}", Sha256::digest(&bytes)));
        }
        assert!(!cli(&args).status.success());
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 1);
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn existing_files_directories_symlinks_and_fifos_are_never_clobbered() {
        let output = private();
        let outside = private();
        fs::write(outside.path().join("secret"), b"outside-artwork").unwrap();
        fs::write(output.path().join("file"), b"kept").unwrap();
        fs::create_dir(output.path().join("directory")).unwrap();
        symlink(outside.path(), output.path().join("link")).unwrap();
        let fifo = std::ffi::CString::new(as_str(&output.path().join("fifo"))).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let root = PrivateDirectory::open(output.path()).unwrap();
        let verified = VerifiedBundle::render(&art()).unwrap();
        for name in ["file", "directory", "link", "fifo"] {
            assert!(verified.publish(&root, name).is_err());
        }
        assert_eq!(fs::read(output.path().join("file")).unwrap(), b"kept");
        assert_eq!(fs::read(outside.path().join("secret")).unwrap(), b"outside-artwork");
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 4);
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
    }

    #[test]
    fn candidate_special_files_and_links_are_not_copied_or_trusted() {
        let candidate = private();
        let root = PrivateDirectory::open(candidate.path()).unwrap();
        art().save_new(&candidate.path().join(PROJECT_FILE)).unwrap();
        let outside = private();
        art().export_png_new(&outside.path().join("source.png")).unwrap();
        symlink(outside.path().join("source.png"), candidate.path().join(PNG_FILE)).unwrap();
        assert!(VerifiedBundle::capture(&root, &art()).is_err());
        fs::remove_file(candidate.path().join(PNG_FILE)).unwrap();
        fs::hard_link(outside.path().join("source.png"), candidate.path().join(PNG_FILE)).unwrap();
        assert!(VerifiedBundle::capture(&root, &art()).is_err());
        fs::remove_file(candidate.path().join(PNG_FILE)).unwrap();
        art().export_png_new(&candidate.path().join(PNG_FILE)).unwrap();
        let _socket = std::os::unix::net::UnixListener::bind(candidate.path().join("ignored.sock")).unwrap();
        symlink(outside.path(), candidate.path().join("ignored-link")).unwrap();
        let verified = VerifiedBundle::capture(&root, &art()).unwrap();
        let output = private();
        verified.publish(&PrivateDirectory::open(output.path()).unwrap(), "result").unwrap();
        assert_eq!(fs::read_dir(output.path().join("result")).unwrap().count(), 2);
    }

    #[test]
    fn cli_confines_relative_inputs_and_does_not_fall_back_to_path_loading() {
        let input = private();
        let output = private();
        art().save_new(&input.path().join("project.json")).unwrap();
        symlink("project.json", input.path().join("alias.json")).unwrap();
        for relative in ["../project.json", "/etc/passwd", "./project.json", "alias.json"] {
            let result = cli(&["publish-raster-bundle", as_str(input.path()), relative, as_str(output.path()), "result"]);
            assert!(!result.status.success());
            let error: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
            assert_eq!(error["ok"], false);
            assert!(error["error"].as_str().unwrap().starts_with("worker I/O:"));
        }
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
        let result = cli(&["capabilities"]);
        let discovery: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(discovery["worker_io"]["autonomous_execution"], false);
        assert_eq!(discovery["worker_io"]["resource_quotas_enforced_for_offline_publication"], false);
        assert_eq!(discovery["persistent_raster_tasks"]["resource_quotas_enforced"], false);
    }
}
