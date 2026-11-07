use std::path::Path;
use zipzi_project::ProjectImporter;

fn main() {
    let importer = ProjectImporter::default();
    for path in std::env::args().skip(1) {
        let (result, _, report) = importer.import_model3(Path::new(&path));
        if result.status.is_ok() {
            println!(
                "OK\t{}\t{:?}\t{:?}\t{:?}",
                path,
                result.diagnostics,
                result.warnings,
                report.map(|report| report.unimported_attachments)
            );
        } else {
            println!(
                "ERROR\t{}\t{}\t{}",
                path, result.status.code, result.status.message
            );
        }
    }
}
