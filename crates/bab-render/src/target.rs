//! Where a frame is drawn: an offscreen texture, or a window's surface.

use anyhow::{Context, Result, bail};

pub(crate) const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The destination of a frame.
pub(crate) enum Target {
    /// A texture we own and can read back. Used by tests and by the screenshot tools.
    Offscreen {
        texture: wgpu::Texture,
        view: wgpu::TextureView,
    },
    /// A window surface. Frames must be presented, and cannot be read back.
    Surface {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
}

/// A frame acquired for drawing.
pub(crate) enum Frame {
    Offscreen(wgpu::TextureView),
    Surface {
        texture: wgpu::SurfaceTexture,
        view: wgpu::TextureView,
    },
}

impl Frame {
    pub(crate) const fn view(&self) -> &wgpu::TextureView {
        match self {
            Self::Offscreen(view) | Self::Surface { view, .. } => view,
        }
    }
}

impl Target {
    pub(crate) fn offscreen(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bab target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self::Offscreen { texture, view }
    }

    pub(crate) fn surface(
        surface: wgpu::Surface<'static>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let mut config = surface
            .get_default_config(adapter, width.max(1), height.max(1))
            .context("surface is not supported by this adapter")?;

        // The shader writes non-linear values directly, so an sRGB view format would
        // gamma-encode them twice and wash the whole terminal out.
        config.format = config.format.remove_srgb_suffix();
        config.view_formats = vec![config.format];
        // FIFO is always supported and never tears. Latency work belongs in a later
        // pass, where the choice should be measured rather than guessed.
        config.present_mode = wgpu::PresentMode::Fifo;

        // Let a translucent background composite with whatever sits behind the window.
        // The shader emits premultiplied colour, so ask for the matching mode; not
        // every adapter offers it, so fall back rather than fail.
        if surface
            .get_capabilities(adapter)
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
        }

        surface.configure(device, &config);
        Ok(Self::Surface { surface, config })
    }

    pub(crate) const fn format(&self) -> wgpu::TextureFormat {
        match self {
            Self::Offscreen { .. } => TARGET_FORMAT,
            Self::Surface { config, .. } => config.format,
        }
    }

    pub(crate) fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        match self {
            Self::Offscreen { .. } => *self = Self::offscreen(device, width, height),
            Self::Surface { surface, config } => {
                config.width = width.max(1);
                config.height = height.max(1);
                surface.configure(device, config);
            }
        }
    }

    /// Acquire the next frame, or `None` when there is nothing to draw into.
    ///
    /// A hidden or occluded window has no drawable, and a compositor may briefly hand
    /// back nothing while it catches up. Neither is an error: the frame is skipped.
    /// A surface that went outdated or lost is reconfigured and retried once, which is
    /// what a resize or a display change looks like from here.
    pub(crate) fn acquire(&mut self, device: &wgpu::Device) -> Result<Option<Frame>> {
        let (surface, config) = match self {
            Self::Offscreen { view, .. } => return Ok(Some(Frame::Offscreen(view.clone()))),
            Self::Surface { surface, config } => (surface, config),
        };

        for attempt in 0..2 {
            match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                    let view = texture
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    return Ok(Some(Frame::Surface { texture, view }));
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost
                    if attempt == 0 =>
                {
                    surface.configure(device, config);
                }
                // Nothing to draw into, and nothing wrong.
                wgpu::CurrentSurfaceTexture::Occluded
                | wgpu::CurrentSurfaceTexture::Timeout
                | wgpu::CurrentSurfaceTexture::Outdated
                | wgpu::CurrentSurfaceTexture::Lost => return Ok(None),
                wgpu::CurrentSurfaceTexture::Validation => bail!("surface validation failed"),
            }
        }
        Ok(None)
    }
}
