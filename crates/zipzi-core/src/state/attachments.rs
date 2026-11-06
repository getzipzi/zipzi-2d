use super::*;
impl DocumentBuilder {
    pub fn edit_attachment(&mut self, edit: AttachmentEdit) -> Result<(), Status> {
        self.write(|this| {
            let changes = this.content.model().attachment_changes(edit)?;
            let removed: Vec<_> = this
                .package_attachments()
                .iter()
                .filter(|a| !changes.attachments.iter().any(|b| a.id == b.id))
                .map(|a| a.id.clone())
                .collect();
            this.clear_owned_editor_objects(&removed);
            if this.package_attachments() != &changes.attachments {
                this.set_package_attachments(changes.attachments)?;
            }
            if this.motion_groups() != &changes.groups {
                this.set_motion_groups(changes.groups)?;
            }
            if this.motion_roots() != &changes.roots {
                this.set_motion_roots(changes.roots)?;
            }
            if this.model3_settings() != &changes.settings {
                this.set_model3_settings(changes.settings)?;
            }
            Ok(())
        })
    }
    pub fn replace_editor_state(&mut self, mut state: EditorState) -> Result<(), Status> {
        self.write(|this| {
            if let Some(issue) = this
                .content
                .model()
                .editor_state_issues(&state)
                .into_iter()
                .next()
            {
                return Err(issue.status);
            }
            state.objects.retain(|_, f| f.hidden_in_editor || f.locked);
            this.set_editor_state(state)
        })
    }
}

impl DocumentState {
    pub fn attachment(&self, id: &str) -> Option<&PackageAttachment> {
        self.package_attachments().iter().find(|a| a.id == id)
    }
    pub fn attachment_path<'a>(&'a self, reference: &'a AttachmentRef) -> Option<&'a str> {
        match reference {
            AttachmentRef::Resolved { attachment_id } => {
                self.attachment(attachment_id).map(|a| a.path.as_str())
            }
            AttachmentRef::Unresolved { path } => Some(path),
        }
    }
    pub fn attachment_available(&self, reference: &AttachmentRef) -> bool {
        match reference {
            AttachmentRef::Resolved { attachment_id } => self
                .attachment(attachment_id)
                .is_some_and(|a| a.bytes.is_some()),
            AttachmentRef::Unresolved { .. } => false,
        }
    }
}
