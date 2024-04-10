use std::path::PathBuf;

/// Given a vector of components, builds, validates, and returns a test resource path
pub fn get_resource_path(components: Vec<&str>) -> String {
    let mut pathbuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for component in components {
        pathbuf.push(component);
    }
    let path = pathbuf.as_path();
    if ! path.exists() {
        panic!("Test resource file is not found or not readable: {}", path.display());
    }

    return path.to_str().expect("test path was not a valid string").into();
}
