# Porting reference: idioms, crates, graphics

## Language idiom map
| Source construct | Rust |
|---|---|
| C `struct` + free functions | `struct` + `impl`; keep function names as methods |
| C++ class hierarchy / virtual | `enum` for closed sets; `trait` + `Box<dyn T>` only for open sets; prefer composition |
| C++ `shared_ptr` graph, back-pointers | indices into `Vec`/arena (`slotmap`, `generational-arena`, `Vec<T>` + `usize` ids); `Rc<RefCell>` only as last resort |
| raw owning pointer, `new/delete` | `Box<T>` / `Vec<T>` |
| nullable pointer | `Option<T>` / `Option<&T>` |
| `void*` user data | generic param or `enum` |
| C unions / tagged unions | `enum` with data |
| `#define` constants / macros | `const`, `const fn`, generics, `macro_rules!` last |
| `goto`/error codes | `Result<T, E>` + `?`; `thiserror` for libs, `anyhow` for bins |
| global mutable state | pass a `&mut Context`; `thread_local!`/`OnceLock` only for true singletons |
| C# `List<T>`/`Dictionary` | `Vec<T>` / `HashMap` (`BTreeMap` or `indexmap` if iteration order matters!) |
| C# events/delegates | channels, callback `Vec<Box<dyn FnMut>>`, or an event enum queue |
| C# GC cycles (parent↔child) | parent owns children; child stores id, not ref |
| Unity `MonoBehaviour` Update | system functions over component storage (ECS: `bevy_ecs`/`hecs`) or plain structs |
| Python dynamic dicts | typed `struct` + `serde`; `serde_json::Value` only at boundaries |
| JS/TS promises | `async` + `tokio` (native) / `wasm-bindgen-futures` (web) |
| `float` | `f32`; `double` → `f64`. Never mix silently |
| C integer overflow wrap | `wrapping_*`; unchecked overflow in source is UB in C but panics in Rust debug — decide per site |
| signed/unsigned casts | explicit `as` / `try_from`; audit every one |
| `rand()` / `Random` | explicit seeded RNG (`rand_pcg`, `rand_chacha`); to match original sequence, re-implement ITS algorithm bit-for-bit |
| `std::sort` unstable vs stable | `sort_by` (stable) vs `sort_unstable_by`; tie order can change results |
| hash-map iteration order | Rust `HashMap` is randomized — use `BTreeMap`/`IndexMap` when order is observable |

## Library map
| Source | Rust crate |
|---|---|
| SDL2 / GLFW / SFML window+input | `winit` (+ `wgpu`), or `sdl2` crate if exact SDL behavior is needed |
| OpenGL / Vulkan / D3D / Metal rendering | `wgpu` (WGSL shaders) |
| GLM / DirectXMath / Eigen | `glam` (games), `nalgebra` (linear algebra) |
| ImGui | `egui` (+ `egui-wgpu`, `egui-winit`) or `imgui-rs` |
| Qt / GTK / WinForms / WPF | `egui`, `iced`, `slint`, `gtk4-rs`, or `tauri` (web UI shell) |
| Electron / web frontend | `tauri`, or wasm (`leptos`, `yew`, `dioxus`) |
| Box2D / Bullet | `rapier2d` / `rapier3d` (behavior differs! see parity notes) |
| OpenAL / FMOD / miniaudio | `rodio`, `kira`, `cpal` |
| stb_image / SOIL | `image` |
| freetype / stb_truetype | `fontdue`, `ab_glyph`, `cosmic-text`, `glyphon` (wgpu text) |
| assimp / tinyobj / FBX | `gltf`, `tobj` |
| JSON/XML/YAML/INI | `serde_json`, `quick-xml`, `serde_yaml`, `toml`/`ini` |
| zlib / zip | `flate2`, `zip` |
| networking | `tokio`, `quinn`, `laminar`/`renet` |
| threads / job system | `rayon`, `std::thread`, `crossbeam` |
| ECS (Unity DOTS, EnTT, flecs) | `bevy_ecs`, `hecs`, `legion` |
| whole-engine target (Unity/Godot game) | consider `bevy` or `macroquad` instead of raw wgpu |
| Python numpy | `ndarray` |
| pygame | `macroquad` / `bevy` / `winit+wgpu` |
| existing C lib with no Rust equivalent | `bindgen` + `-sys` crate first (to get running), then replace |

## When to use wgpu (decision)
- Original does custom GPU rendering (GL/Vulkan/D3D/Metal/WebGL, custom shaders) → **wgpu**.
- Original is 2D with simple blits → `pixels`, `softbuffer`, or `macroquad`/`egui` — wgpu is overkill.
- Original is CLI / server / library / data processing → **no wgpu**. Don't add a GPU dependency.
- Need game-engine features (scene graph, assets, audio, ECS) and not 1:1 render fidelity → `bevy`.
- GPU compute (CUDA/OpenCL/compute shaders) → wgpu compute pipelines.
- Targets web → wgpu works on WebGPU/WebGL2 via wasm; build with `wasm32-unknown-unknown`.

## Graphics porting notes (wgpu)
- **API/version**: wgpu's API changes almost every release. Pin the version, run `cargo doc -p wgpu --open`/read `~/.cargo/registry/src/*/wgpu-<ver>/` and the crate's `examples/` — do not write from memory. The skill's `assets/wgpu_template` compiles on wgpu 30 + winit 0.30.
- **Shaders**: GLSL/HLSL → WGSL. Mechanical route: `naga` CLI (`cargo install naga-cli`; `naga in.frag out.wgsl`), then hand-fix. Check: no implicit conversions, `let`/`var`, explicit `f32(i)`, `select()` argument order (false, true, cond), no `discard` outside fragment.
- **Coordinate systems**: wgpu NDC z is **[0,1]** (GL is [-1,1]); framebuffer origin top-left, NDC +Y up. Use `glam::Mat4::perspective_rh` (z 0..1) not `_gl`. Flip handling for textures loaded bottom-up.
- **sRGB**: pick an `*Srgb` surface format and keep textures `Rgba8UnormSrgb` for color, `Rgba8Unorm` for data (normal maps). Doubled or missing gamma is the #1 "looks washed out/dark" bug.
- **Uniform layout**: WGSL std140-like alignment: `vec3` aligns to 16. Use `#[repr(C)]` + `bytemuck::{Pod,Zeroable}` and explicit padding; or `encase`.
- **Immediate-mode GL** (`glBegin/glEnd`): batch into a dynamic vertex buffer; write once per frame with `queue.write_buffer`.
- **State machine → pipelines**: GL global state (blend, depth, cull) becomes baked into `RenderPipeline`s. Enumerate state combos used by the original, cache one pipeline per combo.
- **Readback for tests**: render to an offscreen `Texture` (`COPY_SRC`), copy to a `MAP_READ` buffer (row pitch aligned to 256 bytes), save PNG, compare with `parity.sh`.
- **Headless CI**: use llvmpipe/lavapipe (`WGPU_BACKEND=vulkan`) or `wgpu::Backends::GL`; software adapters differ slightly from real GPUs → use tolerance.
- **Resize/minimize**: skip zero-size; reconfigure on `Outdated`.
- **Frame loop**: fixed-timestep simulation (accumulator) separate from rendering; the original's timestep semantics are part of behavior.
