#[cfg(target_os = "linux")]
mod linux {
    use perveance::{workspace, Project};
    use std::fs;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
    use std::os::unix::net::UnixListener;

    #[test]
    fn stages_only_validated_project_data_and_removes_it() {
        let parent = tempfile::tempdir().unwrap();
        let host_secret = parent.path().join("host-secret");
        fs::write(&host_secret, b"host-only-sentinel").unwrap();
        symlink(&host_secret, parent.path().join("outside-link")).unwrap();
        fs::hard_link(&host_secret, parent.path().join("outside-hardlink")).unwrap();
        let _listener = UnixListener::bind(parent.path().join("host.sock")).unwrap();
        let mut p = Project::new(2, 2).unwrap();
        p.set_pixel(0, 0, 0, [9, 8, 7, 0]).unwrap();
        p.add_layer().unwrap();
        p.rename_layer(1, "../../host-secret").unwrap(); // data, not a path
        p.layers[1].visible = false;
        p.set_pixel(1, 1, 1, [10, 20, 30, 255]).unwrap();
        let revision = p.revision().unwrap();
        let staged = workspace::stage_project(&p, parent.path()).unwrap();
        let project_path = staged.project_path();
        let dir = project_path.parent().unwrap().to_owned();
        assert_eq!(staged.revision(), revision);
        assert_eq!(project_path.file_name().unwrap(), workspace::STAGED_PROJECT_NAME);
        let entries = fs::read_dir(&dir).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name(), workspace::STAGED_PROJECT_NAME);
        let dir_metadata = fs::symlink_metadata(&dir).unwrap();
        let file_metadata = fs::symlink_metadata(&project_path).unwrap();
        assert!(dir_metadata.file_type().is_dir());
        assert_eq!(dir_metadata.permissions().mode() & 0o077, 0);
        assert!(file_metadata.file_type().is_file());
        assert_eq!(file_metadata.permissions().mode() & 0o077, 0);
        assert_eq!(file_metadata.nlink(), 1);
        assert_eq!(Project::load(&project_path).unwrap(), p);
        assert_eq!(fs::read(&host_secret).unwrap(), b"host-only-sentinel");
        staged.close().unwrap();
        assert!(!dir.exists());
        assert_eq!(fs::read(&host_secret).unwrap(), b"host-only-sentinel");
        assert!(parent.path().join("host.sock").exists());
    }

    #[test]
    fn staging_failure_and_drop_do_not_leave_normal_temporary_directories() {
        let parent = tempfile::tempdir().unwrap();
        let mut invalid = Project::new(1, 1).unwrap();
        invalid.layers[0].pixels.clear();
        assert!(workspace::stage_project(&invalid, parent.path()).is_err());
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
        let p = Project::new(1, 1).unwrap();
        assert!(workspace::stage_project(&p, &parent.path().join("missing")).is_err());
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
        let staged = workspace::stage_project(&p, parent.path()).unwrap();
        let dir = staged.project_path().parent().unwrap().to_owned();
        assert!(dir.exists());
        drop(staged);
        assert!(!dir.exists());
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
    }
}
