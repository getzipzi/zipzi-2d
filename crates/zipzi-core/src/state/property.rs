use super::*;
pub use zipzi_contract::kernel_commands::PropertyPatch;

impl DocumentBuilder {
    pub fn patch_property(&mut self, patch: PropertyPatch) -> Result<(), Status> {
        self.write(|b| {
            let missing = |id: &str| Status::error("MISSING_PROPERTY_TARGET", id);
            match patch {
                PropertyPatch::Mesh {
                    id,
                    keyform,
                    appearance,
                    draw_order,
                } => {
                    b.authoring_unlocked(&id)?;
                    if let Some((binding, samples)) = keyform {
                        b.authoring_unlocked(&binding)?;
                        let mut value = b
                            .binding(&binding)
                            .filter(|v| v.mesh_id == id)
                            .cloned()
                            .ok_or_else(|| missing(&binding))?;
                        let form = value
                            .keyforms
                            .iter_mut()
                            .find(|f| {
                                f.samples.len() == samples.len()
                                    && f.samples.iter().all(|id| samples.contains(id))
                            })
                            .ok_or_else(|| Status::error("INVALID_KEY_COMBINATION", &binding))?;
                        form.appearance = appearance;
                        form.draw_order = draw_order;
                        b.replace_binding(value)
                    } else {
                        let mut value = b.mesh(&id).cloned().ok_or_else(|| missing(&id))?;
                        value.appearance = appearance;
                        value.draw_order = draw_order;
                        b.replace_mesh(value)
                    }
                }
                PropertyPatch::Parameter {
                    id,
                    minimum,
                    maximum,
                    default_value,
                    decimal_places,
                } => {
                    b.authoring_unlocked(&id)?;
                    let mut v = b.parameter(&id).cloned().ok_or_else(|| missing(&id))?;
                    v.minimum = minimum;
                    v.maximum = maximum;
                    v.default_value = default_value;
                    v.decimal_places = decimal_places;
                    b.replace_parameter(v)
                }
                PropertyPatch::Part { id, draw_order } => {
                    b.authoring_unlocked(&id)?;
                    let mut v = b.part(&id).cloned().ok_or_else(|| missing(&id))?;
                    v.draw_order = draw_order;
                    b.replace_part(v)
                }
                PropertyPatch::Glue { id, intensity } => {
                    b.authoring_unlocked(&id)?;
                    let mut v = b.glue(&id).cloned().ok_or_else(|| missing(&id))?;
                    v.intensity = intensity;
                    b.replace_glue(v)
                }
                PropertyPatch::Motion {
                    id,
                    duration,
                    fps,
                    fade_in,
                    fade_out,
                } => {
                    b.content.model().motion_unlocked(&id)?;
                    let mut v = b.motion(&id).cloned().ok_or_else(|| missing(&id))?;
                    v.duration = duration;
                    v.fps = fps;
                    v.fade_in = fade_in;
                    v.fade_out = fade_out;
                    b.replace_motion(v)
                }
                PropertyPatch::Expression {
                    id,
                    fade_in,
                    fade_out,
                } => {
                    b.authoring_unlocked(&id)?;
                    let mut v = b.expression(&id).cloned().ok_or_else(|| missing(&id))?;
                    v.fade_in = fade_in;
                    v.fade_out = fade_out;
                    b.replace_expression(v)
                }
                PropertyPatch::Export {
                    padding_pixels,
                    minimum_extent,
                } => {
                    let mut v = b.setup();
                    v.export.padding_pixels = padding_pixels;
                    v.export.minimum_extent = minimum_extent;
                    b.set_setup(v)
                }
            }
        })
    }
}
