use std::{fs, os::windows::fs::MetadataExt};

/// FILE_ATTRIBUTE_REPARSE_POINT covers symlinks and junctions.
pub(crate) fn is_link(metadata: &fs::Metadata) -> bool {
    metadata.file_attributes() & 0x400 != 0
}
