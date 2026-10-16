use super::*;

impl Exporter<'_> {
    pub(super) fn parameters(&mut self) -> (N, N, N) {
        let display = self.doc.display_info();
        let groups = display.parameter_groups.as_deref().unwrap_or(&[]);
        let mut parameter_parent: HashMap<String, String> = HashMap::new();
        if let Some(entries) = &display.parameters {
            for entry in entries {
                if let kasane_core::document::CdiParameterEntry::Resolved {
                    parameter_id,
                    group_id: Some(group),
                    ..
                } = entry
                {
                    parameter_parent.insert(parameter_id.clone(), group.clone());
                }
            }
        }
        let mut sources = Vec::new();
        let mut child_guids: HashMap<String, Vec<N>> = HashMap::new();
        for id in self.doc.parameter_order() {
            let p = self.doc.get_parameter(id).unwrap();
            let parent = parameter_parent
                .get(id)
                .map_or(ROOT_PARAMETER_GROUP, |v| v.as_str());
            let g = guid("CParameterGuid", id);
            child_guids
                .entry(parent.into())
                .or_default()
                .push(g.clone());
            sources.push(
                N::new("CParameterSource")
                    .field("decimalPlaces", i(p.decimal_places))
                    .field("guid", g)
                    .field("snapEpsilon", f(0.001))
                    .field("minValue", f(p.minimum))
                    .field("maxValue", f(p.maximum))
                    .field("defaultValue", f(p.default_value))
                    .field("isRepeat", b(p.repeat))
                    .field("id", ident("CParameterId", &p.runtime_id))
                    .field(
                        "paramType",
                        N::enumeration(
                            "Type",
                            if p.kind == ParameterKind::BlendShape {
                                "MORPH_TARGET"
                            } else {
                                "NORMAL"
                            },
                        ),
                    )
                    .field("name", s(&p.name))
                    .field("description", s(""))
                    .field("combined", b(false))
                    .field("parentGroupGuid", guid("CParameterGroupGuid", parent)),
            );
        }
        for group in groups {
            child_guids
                .entry(
                    group
                        .parent_id
                        .as_deref()
                        .unwrap_or(ROOT_PARAMETER_GROUP)
                        .into(),
                )
                .or_default()
                .push(guid("CParameterGroupGuid", &group.id));
        }
        let mut make_group = |id: &str, runtime_id: &str, name: &str, parent: Option<&str>| {
            N::new("CParameterGroup")
                .field("name", s(name))
                .field("description", s(""))
                .field("folderIsOpened", b(false))
                .field("guid", guid("CParameterGroupGuid", id))
                .field(
                    "parentGroupGuid",
                    parent.map_or(N::null(), |id| guid("CParameterGroupGuid", id)),
                )
                .field(
                    "_childGuids",
                    N::list("carray_list", child_guids.remove(id).unwrap_or_default()),
                )
                .field("id", ident("CParameterGroupId", runtime_id))
                .field("labelColor", label())
        };
        let root = self.graph.add(make_group(
            ROOT_PARAMETER_GROUP,
            "ParamGroupRoot",
            "Root Parameter Group",
            None,
        ));
        let mut group_nodes = vec![root.clone()];
        for group in groups {
            group_nodes.push(self.graph.add(make_group(
                &group.id,
                &group.runtime_id,
                &group.name,
                Some(group.parent_id.as_deref().unwrap_or(ROOT_PARAMETER_GROUP)),
            )));
        }
        (
            N::new("CParameterSourceSet").field("_sources", N::list("carray_list", sources)),
            N::new("CParameterGroupSet").field("_groups", N::list("carray_list", group_nodes)),
            root,
        )
    }
}
