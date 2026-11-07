//! Colour management with OpenColorIO (the `ocio` crate): one config for the
//! whole program, `$OCIO`'s when it names one that loads, else Imago's own
//! (`imago.ocio`, embedded), and what is drawn turned from the scene's colour
//! space into a display's view: on the CPU for images (`Colour::cpu`), as WGSL
//! for the Viewer (`Colour::shader`).
//!
//! The scene's colour space is USD's default, linear Rec.709: the config's
//! space of that name or alias, else its `scene_linear` role; its colours are
//! turned into the working space on the way to the display. A render is in the
//! config's rendering space (ACEScg in Imago's), and an image can be in any of
//! its spaces.

use std::sync::{Arc, OnceLock};

use ocio::gpu::{GpuShaderCreator, GpuShaderDesc};
use ocio::GpuLanguage;

/// Imago's own config.
pub const IMAGO_CONFIG: &str = include_str!("../imago.ocio");

/// USD's default colour space, by the names and aliases configs give it.
const SCENE_NAMES: [&str; 5] = [
    "lin_rec709_scene",
    "Linear Rec.709 (sRGB)",
    "lin_rec709",
    "Utility - Linear - sRGB",
    "lin_srgb",
];

/// Where the config came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The file `$OCIO` names.
    Env(String),
    /// Imago's own.
    Imago,
}

/// A display and one of its views.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct View {
    pub display: String,
    pub view: String,
}

/// The config, and what Imago reads from it.
pub struct Colour {
    config: ocio::Config,
    pub source: Source,
    /// The config's colour space the scene is in.
    pub scene: String,
    /// Why `$OCIO`'s config is not the one used, or why the scene's space is
    /// the `scene_linear` role: shown where the config is.
    pub problem: Option<String>,
}

/// A transform for the CPU: RGBA pixels in place.
#[derive(Clone)]
pub struct Cpu(Arc<ocio::CpuProcessor>);

impl Cpu {
    pub fn apply(&self, pixels: &mut [[f32; 4]]) {
        self.0.apply_pixels(pixels);
    }
}

/// A transform as WGSL: `text` defines `fn <function>(c: vec4<f32>) ->
/// vec4<f32>`, reading its resources from bind group `group`: a uniform buffer
/// (`uniforms`, at its binding) and LUT textures, read with `textureLoad`.
#[derive(Clone, Debug)]
pub struct Shader {
    pub text: String,
    pub function: String,
    pub group: u32,
    pub uniforms: Option<(u32, Vec<u8>)>,
    pub textures: Vec<Texture>,
}

/// A LUT texture of a `Shader`, as its texels' bytes.
#[derive(Clone, Debug)]
pub struct Texture {
    pub binding: u32,
    /// 1, 2 or 3.
    pub dimensions: u32,
    pub size: [u32; 3],
    /// `Rgba32Float`, else `R32Float`.
    pub rgba: bool,
    pub bytes: Vec<u8>,
    pub bytes_per_row: u32,
}

impl Colour {
    /// The program's config, loaded once.
    pub fn get() -> &'static Colour {
        static COLOUR: OnceLock<Colour> = OnceLock::new();
        COLOUR.get_or_init(|| Colour::load(std::env::var("OCIO").ok().as_deref()))
    }

    /// The config `ocio` names (a file or an `ocio://` built-in), else Imago's
    /// own: with no name, or one that does not load, the problem kept.
    pub fn load(ocio: Option<&str>) -> Colour {
        let named = ocio.map(str::trim).filter(|s| !s.is_empty());
        let mut problem = None;
        if let Some(path) = named {
            match ocio::Config::create_from_file(path) {
                Ok(config) => return Colour::with(config, Source::Env(path.to_string()), None),
                Err(e) => problem = Some(format!("$OCIO ({path}) did not load: {e}. Imago's own config is used")),
            }
        }
        let config = ocio::Config::create_from_str(IMAGO_CONFIG).expect("Imago's config loads");
        Colour::with(config, Source::Imago, problem)
    }

    fn with(config: ocio::Config, source: Source, mut problem: Option<String>) -> Colour {
        let scene = match SCENE_NAMES.iter().find_map(|n| config.get_color_space(n)) {
            Some(cs) => cs.name().to_string(),
            None => {
                let role = config.role_color_space("scene_linear").to_string();
                problem.get_or_insert(format!(
                    "The config has no linear Rec.709 space: the scene is taken as its scene_linear ({role})"
                ));
                role
            }
        };
        Colour {
            config,
            source,
            scene,
            problem,
        }
    }

    /// The config's name, as it says it.
    pub fn name(&self) -> &str {
        self.config.name()
    }

    /// Its displays, in order.
    pub fn displays(&self) -> Vec<String> {
        (0..self.config.num_displays())
            .map(|i| self.config.display(i))
            .collect()
    }

    /// A display's views, in order.
    pub fn views(&self, display: &str) -> Vec<String> {
        (0..self.config.num_views(display))
            .map(|i| self.config.view(display, i))
            .collect()
    }

    /// The config's default display and its default view.
    pub fn default_view(&self) -> View {
        let display = self.config.default_display();
        let view = self.config.default_view(&display);
        View { display, view }
    }

    /// `v` when the config has it, else its default.
    pub fn valid(&self, v: &View) -> View {
        if !v.display.is_empty() && self.config.has_view(&v.display, &v.view) {
            v.clone()
        } else {
            self.default_view()
        }
    }

    /// The space renders are in: the `rendering` role, else `scene_linear`.
    pub fn rendering(&self) -> String {
        let r = self.config.role_color_space("rendering");
        if r.is_empty() {
            self.config.role_color_space("scene_linear").to_string()
        } else {
            r.to_string()
        }
    }

    /// A display's view `view` if it has one, else its default.
    pub fn on_display(&self, display: &str, view: &str) -> View {
        let view = if self.config.has_view(display, view) {
            view.to_string()
        } else {
            self.config.default_view(display)
        };
        View {
            display: display.to_string(),
            view,
        }
    }

    /// Its colour spaces, in order.
    pub fn spaces(&self) -> Vec<String> {
        (0..self.config.num_color_spaces())
            .map(|i| self.config.color_space_name_by_index(i).to_string())
            .collect()
    }

    /// From colour space `from` (empty: the scene's) to `v`.
    fn processor(&self, from: &str, v: &View) -> Result<ocio::Processor, String> {
        let v = self.valid(v);
        // Raw is data: OpenColorIO would pass it through untouched, view and
        // all. A viewer always applies its view, so raw values are shown as if
        // in the scene's space, through the view chosen; to see them as they
        // are, choose the Raw view too.
        let raw = from.eq_ignore_ascii_case("raw") || from.ends_with("- Raw");
        let from = if from.is_empty() || raw { self.scene.as_str() } else { from };
        self.config
            .get_display_view_processor(from, &v.display, &v.view)
            .map_err(|e| format!("{from} to {} / {}: {e}", v.display, v.view))
    }

    /// From colour space `from` to `to` (either empty: the scene's), for the
    /// CPU: an image read in its own space brought into the scene's, as a Nuke
    /// Read node does. A display space (`sRGB - Display`) comes through the
    /// config's view transform, undone, so it shows as it did.
    pub fn convert(&self, from: &str, to: &str) -> Result<Cpu, String> {
        let scene = |s: &str| if s.is_empty() { self.scene.clone() } else { s.to_string() };
        let (from, to) = (scene(from), scene(to));
        let p = self
            .config
            .get_processor(&from, &to)
            .map_err(|e| format!("{from} to {to}: {e}"))?;
        Ok(Cpu(Arc::new(p.default_cpu_processor())))
    }

    /// Colour in space `from` (empty: the scene's) as `v` shows it, for the
    /// CPU.
    pub fn cpu(&self, from: &str, v: &View) -> Result<Cpu, String> {
        Ok(Cpu(Arc::new(self.processor(from, v)?.default_cpu_processor())))
    }

    /// Colour in space `from` (empty: the scene's) as `v` shows it, as WGSL:
    /// `function`, its resources in bind group `group` from binding 0.
    pub fn shader(&self, from: &str, v: &View, function: &str, group: u32) -> Result<Shader, String> {
        let gpu = self
            .processor(from, v)?
            .default_gpu_processor()
            .map_err(|e| e.to_string())?;
        let mut desc = GpuShaderDesc::new();
        desc.set_language(GpuLanguage::Wgsl);
        desc.set_function_name(function);
        desc.set_resource_prefix(&format!("{function}_"));
        desc.set_descriptor_set_index(group, 1).map_err(|e| e.to_string())?;
        gpu.extract_gpu_shader_info(&mut desc).map_err(|e| e.to_string())?;
        let bindings = desc.wgsl_bindings().ok_or("no WGSL bindings")?.clone();
        let uniforms = match bindings.uniform_binding {
            Some(b) => Some((b, bindings.uniform_buffer(&desc).map_err(|e| e.to_string())?)),
            None => None,
        };
        let mut values: Vec<(String, Vec<f32>)> = vec![];
        for i in 0..desc.num_textures() {
            let t = desc.texture(i).map_err(|e| e.to_string())?;
            values.push((t.texture_name.clone(), t.values.clone()));
        }
        for i in 0..desc.num_3d_textures() {
            let t = desc.texture_3d(i).map_err(|e| e.to_string())?;
            values.push((t.texture_name.clone(), t.values.clone()));
        }
        let mut textures = vec![];
        for t in &bindings.textures {
            let data = values
                .iter()
                .find(|(n, _)| *n == t.texture_name)
                .ok_or_else(|| format!("no values for {}", t.texture_name))?;
            textures.push(Texture {
                binding: t.binding,
                dimensions: t.dimensions,
                size: [t.width, t.height, t.depth],
                rgba: matches!(t.format, ocio::gpu::wgsl::WgslTextureFormat::Rgba32Float),
                bytes: t.texel_data(&data.1).map_err(|e| e.to_string())?,
                bytes_per_row: t.bytes_per_row(),
            });
        }
        Ok(Shader {
            text: desc.shader_text().to_string(),
            function: function.to_string(),
            group,
            uniforms,
            textures,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(c: &Colour, v: &View, rgb: [f32; 3]) -> [f32; 3] {
        let mut p = [[rgb[0], rgb[1], rgb[2], 1.0]];
        c.cpu("", v).unwrap().apply(&mut p);
        [p[0][0], p[0][1], p[0][2]]
    }

    /// Imago's config: three displays, ACES first, ACEScg the working and
    /// rendering space, the scene in linear Rec.709; middle grey through
    /// sRGB's curve untone-mapped, ACES's tone scale bringing a bright red
    /// down below 1.
    #[test]
    fn imagos_config_shows_the_scene_in_aces() {
        let c = Colour::load(None);
        assert_eq!((c.source.clone(), c.problem.clone()), (Source::Imago, None));
        assert_eq!(c.scene, "Linear Rec.709 (sRGB)");
        assert_eq!(c.rendering(), "ACEScg");
        assert_eq!(
            c.displays(),
            ["sRGB - Display", "Rec.1886 Rec.709 - Display", "Display P3 - Display"]
        );
        assert_eq!(
            c.views("Rec.1886 Rec.709 - Display"),
            ["ACES 1.0 - SDR Video", "Un-tone-mapped", "Raw"]
        );
        let v = c.default_view();
        assert_eq!(
            (v.display.as_str(), v.view.as_str()),
            ("sRGB - Display", "ACES 1.0 - SDR Video")
        );
        let hot = shown(&c, &v, [4.0, 1.0, 0.2]);
        assert!(hot.iter().all(|x| *x > 0.0 && *x < 1.0), "{hot:?}");
        let plain = c.on_display("sRGB - Display", "Un-tone-mapped");
        let grey = shown(&c, &plain, [0.18; 3]);
        assert!((grey[0] - 0.4614).abs() < 1e-3, "{grey:?}");
        let raw = c.on_display("sRGB - Display", "Raw");
        assert_eq!(shown(&c, &raw, [4.0, 1.0, 0.2]), [4.0, 1.0, 0.2]);
        // A display without the view: its default.
        assert_eq!(
            c.on_display("Display P3 - Display", "Nope").view,
            "ACES 1.0 - SDR Video"
        );
        // ACEScg's green is more saturated than Rec.709's: shown from ACEScg,
        // a pure green leaves sRGB's gamut.
        let mut p = [[0.0, 1.0, 0.0, 1.0]];
        c.cpu("ACEScg", &plain).unwrap().apply(&mut p);
        assert!(p[0][0] < 0.0 && p[0][1] > 1.0, "{p:?}");
    }

    /// A view the config does not have is its default; `$OCIO` naming a
    /// file that is not there gives Imago's config, and says so.
    /// Raw input goes through the view as the scene's colour would: a viewer
    /// always applies its view. Only the Raw view shows values as they are.
    #[test]
    fn raw_input_still_goes_through_the_view() {
        let c = Colour::load(None);
        let srgb = c.on_display("sRGB - Display", "Un-tone-mapped");
        let through = |from: &str, v: &View| {
            let mut p = [[0.18f32, 0.18, 0.18, 1.0]];
            c.cpu(from, v).unwrap().apply(&mut p);
            p[0][0]
        };
        assert!((through("Raw", &srgb) - through("", &srgb)).abs() < 1e-6, "raw is shown as the scene's colour");
        assert!((through("Raw", &srgb) - 0.18).abs() > 0.1, "with the view's curve, not as it is");
        let raw_view = c.on_display("sRGB - Display", "Raw");
        assert!((through("Raw", &raw_view) - 0.18).abs() < 1e-4, "the Raw view shows it as it is");
    }

    #[test]
    fn what_is_missing_falls_back() {
        let c = Colour::load(Some("/nowhere/config.ocio"));
        assert_eq!(c.source, Source::Imago);
        assert!(c.problem.as_deref().is_some_and(|p| p.contains("/nowhere/config.ocio")));
        let v = View {
            display: "Nope".into(),
            view: "Nope".into(),
        };
        assert_eq!(c.valid(&v), c.default_view());
    }

    /// Every view of Imago's config is a WGSL function of the name asked
    /// for, with its resources in the group asked for.
    #[test]
    fn every_view_is_a_shader() {
        let c = Colour::load(None);
        for d in c.displays() {
            for view in c.views(&d) {
                let v = View {
                    display: d.clone(),
                    view,
                };
                let s = c.shader("", &v, "imago_display", 1).unwrap();
                assert!(
                    s.text.contains("fn imago_display(inPixel: vec4<f32>) -> vec4<f32>"),
                    "{v:?}"
                );
                assert_eq!(s.group, 1);
                assert!(s.textures.iter().all(|t| t.binding >= 1));
            }
        }
    }

    /// A built-in config named by `$OCIO` is used, its scene space found by
    /// alias.
    #[test]
    fn a_builtin_config_is_found_by_its_uri() {
        let c = Colour::load(Some("ocio://cg-config-v2.2.0_aces-v1.3_ocio-v2.4"));
        assert_eq!(
            c.source,
            Source::Env("ocio://cg-config-v2.2.0_aces-v1.3_ocio-v2.4".into())
        );
        assert_eq!(c.scene, "Linear Rec.709 (sRGB)");
        assert_eq!(c.default_view().view, "ACES 1.0 - SDR Video");
        assert!(c.shader("", &c.default_view(), "imago_display", 1).is_ok());
    }
}
