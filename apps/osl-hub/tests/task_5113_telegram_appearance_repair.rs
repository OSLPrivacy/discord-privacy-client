use osl_privacy_hub::native_telegram_adapter::{
    repair_telegram_appearance_before_paint, TelegramAppearanceRepairFailure,
    TelegramAppearanceRepairPort, TelegramAppearanceSample, TelegramMeasuredPaint,
    TelegramPaintSurface, TelegramSelectorError, TelegramStructuralPaintTarget,
};
use task_5104_appearance_fingerprint::{
    AppearanceGuard, CaptureObservation, ExposedTextAttributes, Rect, Scope, SurfaceKind,
    WarmUiaObservation,
};

#[derive(Clone, Copy)]
enum Resolver {
    One,
    Zero,
    Multiple,
}

struct Port {
    resolver: Resolver,
    samples: Vec<TelegramAppearanceSample>,
    sample_at: usize,
    comparison: bool,
    stale_apply: bool,
    hidden: bool,
    overlay_pixels: usize,
    paint_actions: usize,
    comparison_actions: usize,
    refusal: Vec<TelegramAppearanceRepairFailure>,
}

impl Port {
    fn good() -> Self {
        let sample = sample();
        Self {
            resolver: Resolver::One,
            samples: vec![sample.clone(), sample],
            sample_at: 0,
            comparison: true,
            stale_apply: false,
            hidden: false,
            overlay_pixels: 3,
            paint_actions: 0,
            comparison_actions: 0,
            refusal: Vec::new(),
        }
    }
}

impl TelegramAppearanceRepairPort for Port {
    fn hide_before_repair(&mut self) {
        self.hidden = true;
        self.overlay_pixels = 0;
    }

    fn resolve_structure(
        &mut self,
        surface: TelegramPaintSurface,
    ) -> Result<TelegramStructuralPaintTarget, TelegramSelectorError> {
        match self.resolver {
            Resolver::One => Ok(TelegramStructuralPaintTarget {
                surface,
                node_index: 4,
            }),
            Resolver::Zero => Err(TelegramSelectorError::Missing),
            Resolver::Multiple => Err(TelegramSelectorError::Ambiguous),
        }
    }

    fn sample(
        &mut self,
        _: &TelegramStructuralPaintTarget,
    ) -> Result<TelegramAppearanceSample, TelegramAppearanceRepairFailure> {
        let sample = self
            .samples
            .get(self.sample_at)
            .cloned()
            .ok_or(TelegramAppearanceRepairFailure::SampleUnavailable)?;
        self.sample_at += 1;
        Ok(sample)
    }

    fn apply_measured(
        &mut self,
        _: &TelegramStructuralPaintTarget,
        paint: &[TelegramMeasuredPaint],
    ) -> Result<(), TelegramAppearanceRepairFailure> {
        if self.stale_apply {
            return Err(TelegramAppearanceRepairFailure::StaleRepaint);
        }
        assert!(paint.iter().all(|entry| matches!(
            entry,
            TelegramMeasuredPaint::ExactPixels(_) | TelegramMeasuredPaint::Value { .. }
        )));
        self.paint_actions += 1;
        self.overlay_pixels = 3;
        Ok(())
    }

    fn local_5103_matches(&mut self, _: &TelegramStructuralPaintTarget) -> bool {
        self.comparison_actions += 1;
        self.comparison
    }

    fn refuse_5105(&mut self, reason: TelegramAppearanceRepairFailure) {
        self.overlay_pixels = 0;
        if self.refusal.is_empty() {
            self.refusal.push(reason);
        }
    }
}

fn sample() -> TelegramAppearanceSample {
    let scope = Scope {
        hwnd: 14,
        hwnd_generation: 7,
        dpi: 144,
        theme: "dark".to_owned(),
        density: "compact".to_owned(),
        zoom_percent: 100,
        surface_kind: SurfaceKind::Composer,
        carrier_version: "Telegram 6.0".to_owned(),
    };
    TelegramAppearanceSample {
        uia: WarmUiaObservation {
            scope: scope.clone(),
            runtime_id_sha256: "a".repeat(64),
            bounds: Rect {
                left: 10,
                top: 20,
                right: 210,
                bottom: 70,
            },
            text: ExposedTextAttributes {
                control_type: "Edit".to_owned(),
                is_read_only: false,
                supports_text_pattern: true,
                character_count: 0,
                line_count: 1,
            },
        },
        capture: CaptureObservation {
            scope,
            dominant_rgb: [1, 2, 3],
            edge_rgb: [4, 5, 6],
            corner_mask: 7,
            edge_mask: 8,
            non_text_phash: 9,
            distinct_rgb_colours: 32,
        },
        paint: vec![
            TelegramMeasuredPaint::ExactPixels(vec![1, 2, 3]),
            TelegramMeasuredPaint::Value {
                name: "font-size".to_owned(),
                value: "14px".to_owned(),
            },
        ],
    }
}

fn run(port: &mut Port) -> Result<(), TelegramAppearanceRepairFailure> {
    repair_telegram_appearance_before_paint(
        port,
        &mut AppearanceGuard::default(),
        TelegramPaintSurface::Composer,
        100,
    )
}

#[test]
fn geometry_colour_type_radius_and_spacing_drift_repair_before_repaint() {
    for changed in ["geometry", "colour", "type", "radius", "spacing"] {
        let mut port = Port::good();
        let mut changed_sample = sample();
        match changed {
            "geometry" => changed_sample.uia.bounds.right += 1,
            "colour" => changed_sample.capture.dominant_rgb[0] += 1,
            "type" => changed_sample.uia.text.control_type = "Document".to_owned(),
            "radius" => changed_sample.capture.corner_mask += 1,
            "spacing" => changed_sample.capture.edge_mask += 1,
            _ => unreachable!(),
        }
        port.samples = vec![changed_sample.clone(), changed_sample];
        assert_eq!(run(&mut port), Ok(()), "{changed}");
        assert!(port.hidden, "{changed}");
        assert_eq!(port.paint_actions, 1, "{changed}");
        assert_eq!(port.comparison_actions, 1, "{changed}");
        assert_eq!(port.refusal.len(), 0, "{changed}");
    }
}

#[test]
fn zero_or_multiple_structure_and_missing_or_unstable_samples_refuse_without_paint() {
    for resolver in [Resolver::Zero, Resolver::Multiple] {
        let mut port = Port::good();
        port.resolver = resolver;
        assert_eq!(
            run(&mut port),
            Err(TelegramAppearanceRepairFailure::Structural)
        );
        assert!(port.hidden);
        assert_eq!(port.overlay_pixels, 0);
        assert_eq!(port.paint_actions, 0);
        assert_eq!(port.comparison_actions, 0);
        assert_eq!(port.refusal.len(), 1);
    }
    let mut starved = Port::good();
    starved.samples.pop();
    assert_eq!(
        run(&mut starved),
        Err(TelegramAppearanceRepairFailure::SampleUnavailable)
    );
    assert_eq!(starved.overlay_pixels, 0);
    assert_eq!(starved.paint_actions, 0);
    assert_eq!(starved.refusal.len(), 1);

    let mut unstable = Port::good();
    unstable.samples[1].capture.edge_rgb[0] += 1;
    assert_eq!(
        run(&mut unstable),
        Err(TelegramAppearanceRepairFailure::SamplesUnstable)
    );
    assert_eq!(unstable.overlay_pixels, 0);
    assert_eq!(unstable.paint_actions, 0);
    assert_eq!(unstable.refusal.len(), 1);
}

#[test]
fn local_5103_failure_and_stale_repaint_exit_with_a_single_5105_refusal() {
    let mut mismatch = Port::good();
    mismatch.comparison = false;
    assert_eq!(
        run(&mut mismatch),
        Err(TelegramAppearanceRepairFailure::Local5103Comparison)
    );
    assert_eq!(mismatch.overlay_pixels, 0);
    assert_eq!(
        mismatch.refusal,
        vec![TelegramAppearanceRepairFailure::Local5103Comparison]
    );

    let mut stale = Port::good();
    stale.stale_apply = true;
    assert_eq!(
        run(&mut stale),
        Err(TelegramAppearanceRepairFailure::StaleRepaint)
    );
    assert_eq!(stale.overlay_pixels, 0);
    assert_eq!(stale.paint_actions, 0);
    assert_eq!(
        stale.refusal,
        vec![TelegramAppearanceRepairFailure::StaleRepaint]
    );
}
