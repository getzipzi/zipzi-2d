//! Real adapter test: rendering, opacity, visibility and resizing.
use editor_scene::{Document, SceneRenderer, TargetConfig};

#[test]
fn renders_edits_on_gpu() -> anyhow::Result<()> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let config = TargetConfig {
        width: 256,
        height: 256,
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let mut scene = SceneRenderer::new(&device, &queue, config)?;
    let sample = |scene: &mut SceneRenderer, document, side: u32| -> anyhow::Result<Vec<u8>> {
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test.output"),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        scene.draw(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            TargetConfig {
                width: side,
                height: side,
                ..config
            },
            document,
        )?;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (side * side * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side * 4),
                    rows_per_image: Some(side),
                },
            },
            target.size(),
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            tx.send(result).unwrap();
        });
        device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })?;
        rx.recv_timeout(std::time::Duration::from_secs(10))??;
        let pixels = buffer.get_mapped_range(..).to_vec();
        buffer.unmap();
        Ok(pixels)
    };
    let normal = sample(&mut scene, Document::default(), 256)?;
    let center = ((128 * 256 + 128) * 4) as usize;
    assert_eq!(&normal[center..center + 4], &[100, 205, 230, 255]);
    assert_eq!(&normal[..4], &[0, 0, 0, 0]);
    let faded = sample(
        &mut scene,
        Document {
            opacity: 0.5,
            ..Default::default()
        },
        256,
    )?;
    assert!((faded[center + 3] as i16 - 128).abs() <= 1);
    let hidden = sample(
        &mut scene,
        Document {
            visible: false,
            ..Default::default()
        },
        256,
    )?;
    assert!(hidden.iter().all(|b| *b == 0));
    let smaller = sample(
        &mut scene,
        Document {
            zoom: 0.5,
            ..Default::default()
        },
        256,
    )?;
    assert!(
        smaller.chunks_exact(4).filter(|p| p[3] > 0).count()
            < normal.chunks_exact(4).filter(|p| p[3] > 0).count() / 2
    );
    let resized = sample(&mut scene, Document::default(), 128)?;
    assert_eq!(resized[((64 * 128 + 64) * 4 + 3) as usize], 255);
    Ok(())
}
