use egui_kittest::Harness;

/// Renders land under the workspace `target/`, which is gitignored.
pub(crate) fn shot_dir() -> std::path::PathBuf {
    out_dir("uishots")
}

fn out_dir(name: &str) -> std::path::PathBuf {
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("target").join(name);
    std::fs::create_dir_all(&d).expect("create output dir");
    d
}

/// Redirects every on-disk profile path (DB, image cache, esilog, lookup) at a scratch dir.
/// `SpaiApp::build` refuses to open a store headlessly unless this is set, so a test that forgets
/// to call it gets no store rather than the user's live one.
pub(crate) fn scratch_profile() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let d = out_dir("uitest-profile");
        std::env::set_var("EVE_SPAI_DATA_DIR", &d);
    });
}

/// Stands in for the http image loaders. Every portrait, corp logo and ship render in the UI is an
/// `egui::Image::new(<url>)`, and several omit `fit_to_exact_size`, so with no loader at all they
/// collapse to a placeholder and the surrounding layout stops matching the real app.
struct StubImages;

impl egui::load::ImageLoader for StubImages {
    fn id(&self) -> &str {
        concat!(module_path!(), "::StubImages")
    }

    fn load(
        &self,
        _ctx: &egui::Context,
        _uri: &str,
        size_hint: egui::SizeHint,
    ) -> egui::load::ImageLoadResult {
        let [w, h] = match size_hint {
            egui::SizeHint::Size { width, height, .. } => [width as usize, height as usize],
            egui::SizeHint::Width(w) => [w as usize, w as usize],
            egui::SizeHint::Height(h) => [h as usize, h as usize],
            egui::SizeHint::Scale(_) => [64, 64],
        };
        let (w, h) = (w.clamp(1, 512), h.clamp(1, 512));
        let px = vec![egui::Color32::from_rgb(0x3A, 0x4A, 0x5E); w * h];
        Ok(egui::load::ImagePoll::Ready {
            image: std::sync::Arc::new(egui::ColorImage::new([w, h], px)),
        })
    }

    fn forget(&self, _uri: &str) {}

    fn forget_all(&self) {}

    fn byte_size(&self) -> usize {
        0
    }
}

/// Everything the app normally does to its context at startup, minus the machine-dependent and
/// networked parts: no system CJK probe, no http image loaders.
pub(crate) fn prepare(ctx: &egui::Context) {
    crate::theme::install_fonts_opts(ctx, false);
    ctx.add_image_loader(std::sync::Arc::new(StubImages));
    crate::theme::Theme::default().apply(ctx);
}

/// What a scene paints. Widget scenes take a `Ui`; the alert and ping windows take a `Context`,
/// because their viewport callbacks ignore the `Ui` they are handed and open their own
/// `CentralPanel` on the context.
pub(crate) enum Draw {
    Ui(Box<dyn FnMut(&mut egui::Ui)>),
    Ctx(Box<dyn FnMut(&egui::Context)>),
}

pub(crate) struct Scene {
    pub(crate) name: &'static str,
    pub(crate) size: egui::Vec2,
    pub(crate) draw: Draw,
    /// Pointer to hold over the scene once it has settled, for anything that only exists under a
    /// cursor. It stays there: egui carries pointer position across passes.
    pub(crate) pointer: Option<egui::Pos2>,
}

impl Scene {
    pub(crate) fn ui(
        name: &'static str,
        size: impl Into<egui::Vec2>,
        f: impl FnMut(&mut egui::Ui) + 'static,
    ) -> Self {
        Self { name, size: size.into(), draw: Draw::Ui(Box::new(f)), pointer: None }
    }

    pub(crate) fn hovered_at(mut self, pos: impl Into<egui::Pos2>) -> Self {
        self.pointer = Some(pos.into());
        self
    }

    pub(crate) fn ctx(
        name: &'static str,
        size: impl Into<egui::Vec2>,
        f: impl FnMut(&egui::Context) + 'static,
    ) -> Self {
        Self { name, size: size.into(), draw: Draw::Ctx(Box::new(f)), pointer: None }
    }
}

/// Hands a `&mut Ui` to code that only wants it for `ui.ctx()`. Invisible and zero-sized, so it
/// cannot contribute anything to the pass itself.
pub(crate) fn detached_ui(ctx: &egui::Context) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        egui::Id::new("uitest_detached"),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ZERO))
            .invisible(),
    )
}

/// `gpu` attaches the wgpu test renderer, which is only needed for [`shot`]. It picks the CPU
/// (lavapipe) adapter on its own and never creates a surface, so no display is involved.
pub(crate) fn build(scene: &mut Scene, gpu: bool) -> Harness<'_> {
    scratch_profile();
    let pointer = scene.pointer;
    let mut builder = Harness::builder().with_size(scene.size).with_max_steps(8);
    if gpu {
        builder = builder.wgpu();
    }
    let mut first = true;
    let mut harness = match &mut scene.draw {
        Draw::Ui(f) => builder.build_ui(move |ui| {
            if std::mem::take(&mut first) {
                prepare(ui.ctx());
            }
            f(ui);
        }),
        // `build` over `build_ui`: the overlay callbacks open their own `CentralPanel` on the
        // context, so they must not already be inside kittest's central-panel frame.
        #[allow(deprecated)]
        Draw::Ctx(f) => builder.build(move |ctx| {
            if std::mem::take(&mut first) {
                prepare(ctx);
            }
            f(ctx);
        }),
    };
    // `set_fonts` only stashes the new definitions and does not request a repaint, so the first
    // pass laid out with egui's defaults (phosphor icons as tofu). Force passes until it settles.
    harness.run_steps(2);
    let _ = harness.run_ok();
    if let Some(p) = pointer {
        harness.event(egui::Event::PointerMoved(p));
        harness.run_steps(2);
    }
    harness
}

pub(crate) fn shot(harness: &mut Harness<'_>, name: &str) {
    let img = harness.render().expect("render");
    let path = shot_dir().join(format!("{name}.png"));
    img.save(&path).expect("write png");
    println!("shot: {}", path.display());
}

/// Same render with egui's interactive-widget overlay switched on: every click target gets an
/// outline, which is the fastest way to see two of them sitting on top of each other.
pub(crate) fn shot_debug(harness: &mut Harness<'_>, name: &str) {
    harness.ctx.global_style_mut(|s| s.debug.show_interactive_widgets = true);
    harness.run_steps(2);
    shot(harness, &format!("{name}.debug"));
    harness.ctx.global_style_mut(|s| s.debug.show_interactive_widgets = false);
    harness.run_steps(2);
}

/// kittest has `hover_at`/`drag_at`/`drop_at` but no click-by-coordinate, which is the only way to
/// reach custom-painted widgets that emit no AccessKit node.
pub(crate) fn click_at(harness: &Harness<'_>, pos: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
    }
}
