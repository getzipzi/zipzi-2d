//! Common Mesh/Deformer presentation; bindings retain their own domain policies.
use super::*;

pub(super) struct Bindings {
    pub name: PropertySpec,
    pub runtime_id: PropertySpec,
    pub part: PropertySpec,
    pub deformer: PropertySpec,
    pub opacity: PropertySpec,
    pub multiply: PropertySpec,
    pub screen: PropertySpec,
    pub enabled: PropertySpec,
}

impl Bindings {
    pub fn identity(&self) -> [FieldSpec; 4] {
        use FieldEditor::*;
        [
            FieldSpec::new(self.name, "properties.name", Text),
            FieldSpec::new(self.runtime_id, "properties.runtime_id", Text),
            FieldSpec::new(self.part, "properties.part", Select).choices(true, mesh::parts),
            FieldSpec::new(self.deformer, "properties.deformer", Select)
                .choices(true, mesh::deformers),
        ]
    }

    pub fn appearance(&self) -> [FieldSpec; 3] {
        use FieldEditor::*;
        [
            FieldSpec::new(self.opacity, "properties.opacity", Number).percent(),
            FieldSpec::new(self.multiply, "properties.multiply", Color).reset_color(0xffffff),
            FieldSpec::new(self.screen, "properties.screen", Color).reset_color(0x000000),
        ]
    }

    pub fn enabled(&self) -> FieldSpec {
        FieldSpec::new(self.enabled, "properties.enabled", FieldEditor::Toggle)
    }
}
