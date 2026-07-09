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

    /// Acquire the next frame.
    ///
    /// A surface can hand back an outdated or lost texture when the window resizes or
    /// the display changes. Reconfiguring and retrying once is the standard recovery;
    /// anything else is a real error.
    pub(crate) fn acquire(&mut self, device: &wgpu::Device) -> Result<Frame> {
        let (surface, config) = match self {
            Self::Offscreen { view, .. } => return Ok(Frame::Offscreen(view.clone())),
            Self::Surface { surface, config } => (surface, config),
        };

        for attempt in 0..2 {
            match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                    let view = texture
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    return Ok(Frame::Surface { texture, view });
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost
                    if attempt == 0 =>
                {
                    surface.configure(device, config);
                }
                wgpu::CurrentSurfaceTexture::Timeout => bail!("timed out acquiring a frame"),
                // The window is hidden. There is nothing to draw, and that is not a bug.
                wgpu::CurrentSurfaceTexture::Occluded => bail!("surface is occluded"),
                wgpu::CurrentSurfaceTexture::Outdated => bail!("surface is still outdated"),
                wgpu::CurrentSurfaceTexture::Lost => bail!("surface was lost"),
                wgpu::CurrentSurfaceTexture::Validation => bail!("surface validation failed"),
            }
        }
        bail!("could not acquire a frame after reconfiguring the surface")
    }
}
