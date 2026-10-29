use crate::application::file_picker::{FilePicker, FileRequest, SelectedFile};

pub struct NativeFilePicker;

impl FilePicker for NativeFilePicker {
    fn select(&mut self, request: &FileRequest) -> Result<Option<Vec<SelectedFile>>, String> {
        // Filters are hints, as in HTML. MIME wildcard filtering needs a platform MIME registry.
        let mut dialog = rfd::FileDialog::new().set_title("Choose files");
        let extensions: Vec<_> = request
            .accept
            .split(',')
            .map(str::trim)
            .filter_map(|s| s.strip_prefix('.'))
            .filter(|s| !s.is_empty())
            .collect();
        if !extensions.is_empty() && request.accept.split(',').all(|s| s.trim().starts_with('.')) {
            dialog = dialog.add_filter("Accepted extensions", &extensions);
        }
        let paths = if request.multiple {
            dialog.pick_files()
        } else {
            dialog.pick_file().map(|p| vec![p])
        };
        Ok(paths.map(|paths| {
            paths
                .into_iter()
                .map(|path| SelectedFile {
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    path,
                })
                .collect()
        }))
    }
}
