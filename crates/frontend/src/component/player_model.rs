use std::sync::Arc;

use gpui::{
    App, AppContext, AvailableSpace, Bounds, Element, Entity, IntoElement, RenderImage, Size,
    Style, Task, px, size,
};
use image::DynamicImage;
use schema::{minecraft_profile::SkinVariant, unique_bytes::UniqueBytes};

use crate::interface_config::InterfaceConfig;

pub const DEFAULT_YAW: f64 = 22.5;
pub const DEFAULT_PITCH: f64 = 10.5;
pub const DEFAULT_ANIMATION: f64 = 1.0 / 16.0;

struct RenderedPlayerModel {
    image: Arc<RenderImage>,
    skin: UniqueBytes,
    cape: Option<UniqueBytes>,
    variant: SkinVariant,
    yaw: f64,
    pitch: f64,
    animation: f64,
    zoom: f64,
    width: u32,
    height: u32,
}

pub struct PlayerModelState {
    pub skin: UniqueBytes,
    pub cape: Option<UniqueBytes>,
    pub variant: SkinVariant,
    pub yaw: f64,
    pub pitch: f64,
    pub animation: f64,
    rendered: Option<RenderedPlayerModel>,
    render_task: Option<Task<()>>,
    /// Decoded skin/cape images, cached by source bytes so the PNG is not
    /// re-decoded on every frame while rotating. Invalidated automatically
    /// when the bytes change; decode failures simply leave `None` and the
    /// render falls back to `None` (transparent) as before.
    decoded_skin: Option<(UniqueBytes, Arc<DynamicImage>)>,
    decoded_cape: Option<(Option<UniqueBytes>, Option<Arc<DynamicImage>>)>,
}

impl PlayerModelState {
    pub fn new(cx: &mut App, skin: UniqueBytes, variant: SkinVariant) -> Entity<Self> {
        let entity = cx.new(|_| Self {
            skin,
            cape: None,
            variant,
            yaw: DEFAULT_YAW,
            pitch: DEFAULT_PITCH,
            animation: DEFAULT_ANIMATION,
            rendered: None,
            render_task: None,
            decoded_skin: None,
            decoded_cape: None,
        });
        cx.observe_release(&entity, |entity, cx| {
            if let Some(rendered) = entity.rendered.take() {
                cx.drop_image(rendered.image, None);
            }
        })
        .detach();
        entity
    }

    /// Decoded skin image for the current `skin` bytes, decoding once and
    /// reusing the result across frames. Only decodes when the bytes
    /// changed since the last call, so per-frame cost is an Arc clone.
    /// Decoding a 64x64 skin PNG takes microseconds; doing it here (only
    /// on actual skin changes) keeps it off the hot rotation path.
    pub fn skin_image(&mut self) -> Option<Arc<DynamicImage>> {
        if self
            .decoded_skin
            .as_ref()
            .is_some_and(|(bytes, _)| *bytes == self.skin)
        {
            return self.decoded_skin.as_ref().map(|(_, image)| image.clone());
        }
        let image = Arc::new(
            image::load_from_memory_with_format(&self.skin, image::ImageFormat::Png).ok()?,
        );
        self.decoded_skin = Some((self.skin.clone(), image.clone()));
        Some(image)
    }

    /// Decoded cape image for the current `cape` bytes (same caching
    /// contract as [`Self::skin_image`]).
    pub fn cape_image(&mut self) -> Option<Arc<DynamicImage>> {
        if self
            .decoded_cape
            .as_ref()
            .is_some_and(|(bytes, _)| *bytes == self.cape)
        {
            return self
                .decoded_cape
                .as_ref()
                .and_then(|(_, image)| image.clone());
        }
        let image = self
            .cape
            .as_ref()
            .and_then(|cape| {
                image::load_from_memory_with_format(cape, image::ImageFormat::Png).ok()
            })
            .map(Arc::new);
        self.decoded_cape = Some((self.cape.clone(), image.clone()));
        image
    }

    pub fn needs_rerender(&self, width: u32, height: u32, zoom: f64) -> bool {
        let Some(rendered) = &self.rendered else {
            return true;
        };
        return rendered.width != width
            || rendered.height != height
            || rendered.yaw != self.yaw
            || rendered.pitch != self.pitch
            || rendered.animation != self.animation
            || rendered.variant != self.variant
            || rendered.skin != self.skin
            || rendered.cape != self.cape
            || rendered.zoom != zoom;
    }
}

pub struct PlayerModel {
    state: Entity<PlayerModelState>,
}

impl PlayerModel {
    pub fn new(state: &Entity<PlayerModelState>) -> Self {
        Self {
            state: state.clone(),
        }
    }
}

impl IntoElement for PlayerModel {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PlayerModel {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut gpui::Window,
        _cx: &mut gpui::App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let layout_id = window.request_measured_layout(
            Style::default(),
            move |known, available_space, _window, _cx| {
                let height = if let Some(height) = known.height {
                    height
                } else {
                    match available_space.height {
                        AvailableSpace::Definite(pixels) => pixels,
                        AvailableSpace::MinContent => px(0.0),
                        AvailableSpace::MaxContent => px(1000.0),
                    }
                };

                let width = px(height.as_f32() * crate::skin_renderer::ASPECT_RATIO as f32);

                size(width, height)
            },
        );

        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: gpui::Bounds<gpui::Pixels>,
        _element_size: &mut Self::RequestLayoutState,
        _window: &mut gpui::Window,
        _cx: &mut gpui::App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _global_id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        let element_height = bounds.size.height.as_f32().round();
        let element_width =
            (element_height as f32 * crate::skin_renderer::ASPECT_RATIO as f32).round();
        let window_scale = window.scale_factor();
        let image_height = (element_height * window_scale) as u32;
        let image_width = (element_width * window_scale) as u32;
        // Guard against zero-size paints during layout (would create 0-size
        // images / divide-by-zero scale and can panic in image/GPU code).
        if image_width == 0 || image_height == 0 || element_height <= 0.0 || element_width <= 0.0 {
            return;
        }
        let zoom = InterfaceConfig::get(cx).player_model_zoom.clamp(50, 400) as f64 / 100.0;
        self.state.update(cx, |state, cx| {
            if state.render_task.is_none() && state.needs_rerender(image_width, image_height, zoom)
            {
                // Decode once per skin/cape change (cached by bytes); the
                // per-frame background job then only rasterizes.
                // If the skin doesn't decode, don't start (or wedge)
                // `render_task`: the old frame (or nothing) stays up and a
                // later paint retries, so fixing the skin recovers.
                if let Some(skin_image) = state.skin_image() {
                    let cape_image = state.cape_image();
                    let yaw = state.yaw;
                    let pitch = state.pitch;
                    let animation = state.animation;
                    let variant = state.variant;

                    let (send, recv) = tokio::sync::oneshot::channel();

                    cx.background_executor()
                        .spawn(async move {
                            send.send(crate::skin_renderer::render_skin_3d_images(
                                &skin_image,
                                cape_image.as_deref(),
                                variant,
                                image_width,
                                image_height,
                                yaw,
                                pitch,
                                animation,
                                0.0,
                                zoom,
                            ))
                        })
                        .detach();

                    let skin = state.skin.clone();
                    let cape = state.cape.clone();
                    state.render_task = Some(cx.spawn(async move |state, cx| {
                        let Ok(Some(mut data)) = recv.await else {
                            // Don't leave a completed task behind: clear the
                            // slot so later paints can retry the render.
                            _ = state.update(cx, |state, _| {
                                state.render_task = None;
                            });
                            return;
                        };

                        _ = state.update(cx, |state, cx| {
                            for pixel in data.chunks_exact_mut(4) {
                                pixel.swap(0, 2);
                            }

                            let render_image =
                                Arc::new(RenderImage::new([image::Frame::new(data)]));

                            if let Some(rendered) = state.rendered.take() {
                                cx.drop_image(rendered.image, None);
                            }
                            state.rendered = Some(RenderedPlayerModel {
                                image: render_image,
                                skin,
                                cape,
                                variant,
                                yaw,
                                pitch,
                                animation,
                                zoom,
                                width: image_width,
                                height: image_height,
                            });
                            state.render_task = None;
                            cx.notify();
                        });
                    }));
                }
            }

            if let Some(rendered) = &state.rendered {
                let image_bounds = Bounds {
                    origin: bounds.origin,
                    size: Size::new(px(element_width), px(element_height)),
                };
                _ = window.paint_image(
                    image_bounds,
                    image_bounds,
                    Default::default(),
                    rendered.image.clone(),
                    0,
                    false,
                );
            }
        });
    }
}
