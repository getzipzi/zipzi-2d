use editor_core::*;

fn resolve(
    platform: ShortcutPlatform,
    bindings: Vec<ShortcutOverride>,
) -> anyhow::Result<Vec<EffectiveBinding>> {
    ShortcutOverrides { bindings }.resolve(&KeyboardPolicy::for_platform(platform), |key| {
        Ok(key.to_lowercase())
    })
}

fn keys(bindings: &[EffectiveBinding], command: CommandId) -> Vec<&str> {
    bindings
        .iter()
        .filter(|binding| binding.command == command)
        .map(|binding| binding.key.as_str())
        .collect()
}

fn binding(
    command: CommandId,
    platform: Option<ShortcutPlatform>,
    keys: &[&str],
) -> ShortcutOverride {
    ShortcutOverride {
        command,
        platform,
        keys: keys.iter().map(|key| (*key).into()).collect(),
    }
}

#[test]
fn scene_creation_bindings_have_stable_ids_and_can_be_configured_without_defaults()
-> anyhow::Result<()> {
    for (id, key) in [
        (CommandId::CreatePart, "scene.create-part"),
        (CommandId::CreateMesh, "scene.create-mesh"),
        (CommandId::CreateWarp, "scene.create-warp"),
        (CommandId::CreateRotation, "scene.create-rotation"),
        (CommandId::CreateOffscreen, "scene.create-offscreen"),
    ] {
        assert_eq!(serde_json::from_value::<CommandId>(key.into())?, id);
        assert_eq!(serde_json::to_value(id)?, key);
        assert!(keys(&resolve(ShortcutPlatform::Macos, vec![])?, id).is_empty());
        assert_eq!(
            keys(
                &resolve(
                    ShortcutPlatform::Macos,
                    vec![binding(id, None, &["ctrl-alt-n"])]
                )?,
                id
            ),
            ["ctrl-alt-n"]
        );
        assert!(
            resolve(
                ShortcutPlatform::Macos,
                vec![binding(id, None, &["secondary-s"])]
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn palette_bindings_are_configurable_and_conflict_with_document_commands() -> anyhow::Result<()> {
    let defaults = resolve(ShortcutPlatform::Macos, vec![])?;
    assert_eq!(keys(&defaults, CommandId::PaletteMoveUp), ["alt-up"]);
    let configured = resolve(
        ShortcutPlatform::Macos,
        vec![binding(CommandId::PaletteMoveUp, None, &["alt-k"])],
    )?;
    assert_eq!(keys(&configured, CommandId::PaletteMoveUp), ["alt-k"]);
    assert!(
        resolve(
            ShortcutPlatform::Macos,
            vec![binding(CommandId::PaletteMoveUp, None, &["secondary-z"])]
        )
        .is_err()
    );
    assert!(CommandScope::Palette.overlaps(CommandScope::Document));
    assert!(!CommandScope::Palette.overlaps(CommandScope::Timeline));
    assert!(!CommandScope::Palette.overlaps(CommandScope::Property));
    Ok(())
}

#[test]
fn platform_defaults_expand_before_the_host_mapper_and_preserve_real_control() -> anyhow::Result<()>
{
    for platform in [
        ShortcutPlatform::Macos,
        ShortcutPlatform::Windows,
        ShortcutPlatform::Linux,
    ] {
        let bindings = resolve(platform, vec![])?;
        let mac = platform == ShortcutPlatform::Macos;
        assert_eq!(
            keys(&bindings, CommandId::Save),
            [if mac { "cmd-s" } else { "ctrl-s" }]
        );
        assert_eq!(
            keys(&bindings, CommandId::Undo),
            [if mac { "cmd-z" } else { "ctrl-z" }]
        );
        assert_eq!(
            keys(&bindings, CommandId::TimelineCopy),
            [if mac { "cmd-c" } else { "ctrl-c" }]
        );
        assert_eq!(keys(&bindings, CommandId::Next), ["ctrl-tab"]);
        assert_eq!(keys(&bindings, CommandId::Previous), ["ctrl-shift-tab"]);
        assert_eq!(keys(&bindings, CommandId::PlayPause), ["space"]);
        assert_eq!(
            keys(&bindings, CommandId::ZoomOut),
            [if mac { "cmd--" } else { "ctrl--" }]
        );
        assert_eq!(
            keys(&bindings, CommandId::Redo).contains(&"ctrl-y"),
            platform == ShortcutPlatform::Windows
        );
    }
    Ok(())
}

#[test]
fn platform_overrides_win_over_common_overrides_and_empty_lists_unbind() -> anyhow::Result<()> {
    use CommandId::*;
    use ShortcutPlatform::*;
    let overrides = vec![
        binding(Open, None, &["secondary-alt-o"]),
        binding(Open, Some(Windows), &["secondary-shift-o"]),
        binding(Open, Some(Linux), &[]),
        binding(Redo, Some(Windows), &[]),
    ];
    assert_eq!(
        keys(&resolve(Macos, overrides.clone())?, Open),
        ["alt-cmd-o"]
    );
    let windows = resolve(Windows, overrides.clone())?;
    assert_eq!(keys(&windows, Open), ["ctrl-shift-o"]);
    assert!(keys(&windows, Redo).is_empty());
    assert!(keys(&resolve(Linux, overrides)?, Open).is_empty());
    // A common override also replaces all platform defaults, including Ctrl+Y.
    assert_eq!(
        keys(
            &resolve(Windows, vec![binding(Redo, None, &["secondary-alt-z"])])?,
            Redo
        ),
        ["ctrl-alt-z"]
    );
    Ok(())
}

#[test]
fn equivalent_semantic_and_platform_modifiers_conflict() -> anyhow::Result<()> {
    use ShortcutPlatform::*;
    for (platform, alias) in [
        (Macos, "cmd-s"),
        (Macos, "super-s"),
        (Macos, "win-s"),
        (Windows, "ctrl-s"),
        (Linux, "ctrl-s"),
    ] {
        assert!(resolve(platform, vec![binding(CommandId::Open, None, &[alias])]).is_err());
        assert!(
            resolve(
                platform,
                vec![binding(CommandId::Save, None, &["secondary-s", alias])]
            )
            .is_err()
        );
    }
    // A platform key remains distinct from Control outside macOS.
    assert_eq!(
        keys(
            &resolve(Windows, vec![binding(CommandId::Open, None, &["cmd-s"])])?,
            CommandId::Open
        ),
        ["cmd-s"]
    );
    Ok(())
}

#[test]
fn reserved_system_keys_and_text_protection_follow_the_selected_platform() -> anyhow::Result<()> {
    use ShortcutPlatform::*;
    for (platform, key) in [
        (Macos, "cmd-h"),
        (Macos, "alt-super-h"),
        (Macos, "cmd-m"),
        (Windows, "alt-f4"),
        (Linux, "super-l"),
    ] {
        assert!(resolve(platform, vec![binding(CommandId::Open, None, &[key])]).is_err());
    }
    for platform in [Macos, Windows, Linux] {
        for key in ["secondary-c", "secondary-z", "secondary-a", "a"] {
            assert!(resolve(platform, vec![binding(CommandId::Open, None, &[key])]).is_err());
        }
    }
    assert!(resolve(Macos, vec![binding(CommandId::Open, None, &["ctrl-a"])]).is_err());
    assert!(resolve(Windows, vec![binding(CommandId::Open, None, &["cmd-h"])]).is_ok());
    Ok(())
}

#[test]
fn timeline_and_viewport_bindings_are_disjoint_but_document_bindings_overlap() -> anyhow::Result<()>
{
    use CommandId::*;
    use ShortcutPlatform::*;
    let bindings = resolve(Macos, vec![binding(TimelineCopy, None, &["v"])])?;
    assert_eq!(keys(&bindings, SelectTool), ["v"]);
    assert_eq!(keys(&bindings, TimelineCopy), ["v"]);
    assert!(
        resolve(
            Macos,
            vec![
                binding(TimelineCopy, None, &["v"]),
                binding(Undo, None, &["v"])
            ]
        )
        .is_err()
    );
    assert!(resolve(Macos, vec![binding(TimelineCopy, None, &["left"])]).is_err());
    assert!(resolve(Macos, vec![binding(Open, None, &["secondary-c"])]).is_err());
    Ok(())
}

#[test]
fn duplicate_overrides_are_rejected_per_platform_and_config_keeps_semantic_keys()
-> anyhow::Result<()> {
    use CommandId::*;
    use ShortcutPlatform::*;
    assert!(
        resolve(
            Macos,
            vec![binding(Open, None, &[]), binding(Open, None, &[])]
        )
        .is_err()
    );
    assert!(
        resolve(
            Macos,
            vec![
                binding(Open, Some(Windows), &[]),
                binding(Open, Some(Windows), &[])
            ]
        )
        .is_err()
    );
    let config = ShortcutOverrides {
        bindings: vec![binding(Open, Some(Windows), &["secondary-alt-o"])],
    };
    let json = serde_json::to_string(&config)?;
    assert!(json.contains("secondary-alt-o"));
    assert!(json.contains("windows"));
    let restored: ShortcutOverrides = serde_json::from_str(&json)?;
    restored.resolve(&KeyboardPolicy::for_platform(Windows), |key| {
        assert!(!key.contains("secondary"));
        Ok(key.into())
    })?;
    assert_eq!(restored.bindings[0].keys, ["secondary-alt-o"]);
    Ok(())
}
