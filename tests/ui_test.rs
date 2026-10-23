//! Mounts the real launcher view headlessly with fixture data. Always checks that the markup builds
//! and every host action is registered; with `FX_UI_SNAPSHOT_DIR` set it also renders each page to PNG:
//! `FX_UI_SNAPSHOT_DIR=target/ui cargo test --test ui_test -- --nocapture`
#![cfg(feature = "ui")]

use aegle::ui::scene::Affine;
use aegle::ui::{Size, TextSystem, Theme, Ui, Visit};
use aegle_render_software::{Renderer, Surface};
use fxlauncher::account::{Account, AccountStore};
use fxlauncher::config::{LauncherConfig, ThemePreference};
use fxlauncher::net::download::ManifestVersion;
use fxlauncher::net::profile::McProfile;
use fxlauncher::task::{TaskManager, TaskStatus};
use fxlauncher::ui::{Controller, Msg, Post, dark_theme, light_theme, mount};
use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

const SIZE: Size = Size { width: 960.0, height: 600.0 };

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn fixture_game_dir(dir: &Path) {
    let v = dir.join("versions");
    write(&v.join("1.21.1/1.21.1.json"), r#"{"id":"1.21.1","type":"release","mainClass":"net.minecraft.client.main.Main","javaVersion":{"majorVersion":21}}"#);
    write(&v.join("Fabric 1.21.1/Fabric 1.21.1.json"), r#"{"id":"Fabric 1.21.1","inheritsFrom":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
        "libraries":[{"name":"net.fabricmc:fabric-loader:0.16.9","url":"https://maven.fabricmc.net/"}]}"#);
    write(&v.join("1.20.1-Forge/1.20.1-Forge.json"), r#"{"id":"1.20.1-Forge","inheritsFrom":"1.20.1","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
        "libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.3.0"}]}"#);
    let mods = v.join("Fabric 1.21.1/mods");
    for (name, size) in [("sodium-fabric-0.6.0+mc1.21.1.jar", 1_200_000), ("iris-fabric-1.8.0+mc1.21.1.jar", 2_400_000), ("modmenu-11.0.2.jar.disabled", 300_000)] {
        write(&mods.join(name), &"x".repeat(size));
    }
    write(&v.join("Fabric 1.21.1/shaderpacks/ComplementaryReimagined_r5.3.zip"), &"x".repeat(80_000));
    write(&v.join("Fabric 1.21.1/saves/新的世界/level.dat"), &"x".repeat(50_000));
    write(&v.join("Fabric 1.21.1/saves/Survival/level.dat"), &"x".repeat(900_000));
}

fn manifest() -> Vec<ManifestVersion> {
    let mk = |id: &str, ty: &str, date: &str| ManifestVersion {
        id: id.into(),
        version_type: ty.into(),
        url: String::new(),
        sha1: String::new(),
        release_time: format!("{date}T10:00:00+00:00"),
    };
    vec![
        mk("1.21.4", "release", "2024-12-03"),
        mk("24w46a", "snapshot", "2024-11-13"),
        mk("1.21.3", "release", "2024-10-23"),
        mk("1.21.1", "release", "2024-08-08"),
        mk("1.21", "release", "2024-06-13"),
        mk("1.20.6", "release", "2024-04-29"),
        mk("1.20.4", "release", "2023-12-07"),
        mk("1.20.1", "release", "2023-06-12"),
        mk("1.19.4", "release", "2023-03-14"),
        mk("1.18.2", "release", "2022-02-28"),
        mk("1.16.5", "release", "2021-01-15"),
        mk("1.12.2", "release", "2017-09-18"),
    ]
}

const MS_UUID: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";

fn fixture_accounts() -> AccountStore {
    let mut store = AccountStore::default();
    store.upsert(Account::new_offline("Steve"));
    store.upsert(Account::Microsoft {
        username: "Notch".into(),
        uuid: MS_UUID.into(),
        access_token: "token".into(),
        refresh_token: "refresh".into(),
        expires_at: u64::MAX,
    });
    store
}

fn fixture_profile() -> McProfile {
    serde_json::from_str(r#"{
        "id": "069a79f444e94726a5befca90e38aaf5", "name": "Notch",
        "skins": [{"id": "s1", "state": "ACTIVE", "url": "http://textures.minecraft.net/texture/abc", "variant": "CLASSIC"}],
        "capes": [
            {"id": "c1", "state": "ACTIVE", "url": "", "alias": "Migrator"},
            {"id": "c2", "state": "INACTIVE", "url": "", "alias": "Vanilla"}
        ]
    }"#).unwrap()
}

/// A 64×64 Steve-like skin texture as PNG: skin-toned face with hair and eyes, shirt, trousers.
fn fixture_skin_png() -> Vec<u8> {
    let mut px = vec![0u8; 64 * 64 * 4];
    let mut fill = |x0: usize, y0: usize, w: usize, h: usize, rgb: [u8; 3]| {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                px[(y * 64 + x) * 4..][..4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
    };
    let (skin, hair, shirt, pants) = ([0xB4, 0x84, 0x6C], [0x3B, 0x2A, 0x1E], [0x00, 0xA8, 0xA8], [0x3A, 0x3A, 0x9C]);
    fill(8, 8, 8, 8, skin); // face
    fill(8, 8, 8, 2, hair);
    fill(9, 12, 2, 1, [0xFF, 0xFF, 0xFF]); // eyes
    fill(13, 12, 2, 1, [0xFF, 0xFF, 0xFF]);
    fill(10, 12, 1, 1, [0x49, 0x3C, 0xA0]);
    fill(13, 12, 1, 1, [0x49, 0x3C, 0xA0]);
    fill(11, 14, 2, 1, [0x6A, 0x40, 0x30]); // mouth
    fill(20, 20, 8, 12, shirt); // body
    fill(44, 20, 4, 4, shirt); // right arm: sleeve, then hand
    fill(44, 24, 4, 8, skin);
    fill(36, 52, 4, 4, shirt); // left arm
    fill(36, 56, 4, 8, skin);
    fill(4, 20, 4, 12, pants); // legs
    fill(20, 52, 4, 12, pants);
    fill(4, 30, 4, 2, [0x55, 0x55, 0x55]); // shoes
    fill(20, 62, 4, 2, [0x55, 0x55, 0x55]);
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 64, 64);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&px).unwrap();
    out
}

fn fixture_tasks(manager: &TaskManager) {
    let a = manager.create_task("安装实例 Fabric 1.21.1");
    a.set_status(TaskStatus::Running);
    a.set_stage("下载资源文件");
    a.reset_progress(412_000_000, 3888);
    a.transferred.store(187_000_000, Ordering::SeqCst);
    a.done_items.store(1764, Ordering::SeqCst);
    a.speed_bps.store(6_400_000, Ordering::SeqCst);
    let b = manager.create_task("安装模组 Sodium");
    b.set_status(TaskStatus::Completed);
    let c = manager.create_task("安装实例 1.20.1-Forge");
    c.set_stage("运行安装处理器 3/7");
    c.fail("安装处理器 net.minecraftforge:binarypatcher 失败");
}

fn render(ui: &Ui, theme: Theme, path: &Path) {
    ui.refresh().unwrap();
    ui.dispatch_callbacks().unwrap();
    ui.advance_animations(Duration::from_secs(2)).unwrap();
    ui.refresh().unwrap();
    // Value changes ease over a few frames (sliders, progress bars); settle them.
    let start = std::time::Instant::now();
    for i in 1..=20 {
        if !ui.wants_frames() {
            break;
        }
        ui.run_frame(start + Duration::from_millis(100 * i));
        ui.refresh().unwrap();
    }
    let scale = 1.0;
    let (w, h) = ((SIZE.width * scale) as u32, (SIZE.height * scale) as u32);
    let mut pixels = vec![0; w as usize * h as usize * 4];
    let mut surface = Surface::new(&mut pixels, w, h).unwrap();
    let mut renderer = Renderer::default();
    let mut frame = renderer.begin_frame(&mut surface, theme.background);
    let place = Affine::scale(scale, scale);
    ui.visit_scenes(|visit| {
        match visit {
            Visit::Scene { scene, transform, clip } => {
                let clip = clip.map(|c| aegle::ui::scene::Rect::new(c.origin.x * scale, c.origin.y * scale, c.size.width * scale, c.size.height * scale));
                frame.draw_clipped(scene, transform.then(place)?, clip)?
            }
            Visit::PushLayer(layer) => frame.push_layer(&layer.then(place)?)?,
            Visit::PopLayer => frame.pop_layer()?,
        }
        Ok(())
    })
    .unwrap();
    drop(frame);
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(fs::File::create(path).unwrap()), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(surface.data()).unwrap();
    println!("wrote {}", path.display());
}

fn build(theme: Theme, game_dir: &Path) -> (Ui, Rc<Controller>) {
    let fonts = Rc::new(RefCell::new(TextSystem::system()));
    let ui = Ui::with_fonts(fonts, theme).unwrap();
    ui.resize(SIZE);
    let manager = Arc::new(TaskManager::new());
    fixture_tasks(&manager);
    let config = LauncherConfig {
        game_dir: game_dir.to_path_buf(),
        selected_version: "Fabric 1.21.1".into(),
        ..Default::default()
    };
    let post: Post = Arc::new(|_| {});
    let controller = mount(&ui.root(), config, fixture_accounts(), manager, post, false).unwrap();
    controller.handle(Msg::Manifest(Ok(manifest())));
    controller.handle(Msg::Profile { uuid: MS_UUID.into(), result: Ok(fixture_profile()) });
    controller.handle(Msg::SkinTexture { uuid: MS_UUID.into(), result: Ok(fixture_skin_png()) });
    controller.handle(Msg::Tick);
    (ui, controller)
}

#[test]
fn test_ui_mounts_and_renders() {
    let dir = tempfile::tempdir().unwrap();
    fixture_game_dir(dir.path());
    // Keep the test from touching the user's real settings file.
    unsafe { std::env::set_var("HOME", dir.path()) };

    let out = std::env::var_os("FX_UI_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    if let Some(out) = &out {
        fs::create_dir_all(out).unwrap();
    }
    let pages = ["play", "accounts", "instances", "download", "tasks", "settings"];

    let (ui, controller) = build(light_theme(), dir.path());
    let shell = controller.shell();
    assert_eq!(shell.current_title.get(), "Fabric 1.21.1");
    assert_eq!(shell.current_detail.get(), "Fabric 0.16.9 · Minecraft 1.21.1");
    assert_eq!(shell.picked.get(), "Fabric 1.21.1");
    assert_eq!(shell.mods_summary.get(), "3 个模组 · 2 个已启用 · 1 个已禁用");
    assert_eq!(shell.mods_warning.get(), "", "mods built for the instance's version must not warn");
    assert!(shell.task_badge.get().ends_with("45%"), "{}", shell.task_badge.get());
    // Releases only by default; the newest one is preselected for installing.
    let version_rows = match shell.versions.get() {
        aegle::loader::Data::List(rows) => rows.len(),
        _ => 0,
    };
    assert_eq!(version_rows, 11);
    assert_eq!(shell.chosen.get(), "1.21.4");
    assert_eq!(shell.instance_name.text(), "1.21.4");
    // The play dropdown lists every instance and points at the current one.
    assert_eq!(shell.instance_titles.get(), ["1.20.1-Forge", "1.21.1", "Fabric 1.21.1"]);
    assert_eq!(shell.current_index.get(), 2);
    // The signed-in account launches the game; its profile fills the 账户 page.
    assert_eq!(shell.account_titles.get(), ["Steve", "Notch"]);
    assert_eq!(shell.account_index.get(), 1);
    assert_eq!(shell.account.get(), MS_UUID);
    assert!(shell.account_microsoft.get());
    assert!(!shell.cape_none.get() && !shell.skin_slim.get());
    assert_eq!(shell.new_name.text(), "Notch");
    assert!(shell.skin_shown.get(), "the skin texture is drawn as a front view");
    assert_eq!(shell.skin_preview.image().map(|i| (i.width(), i.height())), Some((128, 256)));

    // Show the loader-version dropdown as it looks once Fabric versions arrive.
    shell.loader.set(1).unwrap();
    shell.loader_items.set(vec!["0.16.9（推荐）".into(), "0.16.8".into(), "0.16.7".into()]).unwrap();
    shell.loader_status.set("共 3 个 Fabric 版本".into()).unwrap();
    shell.instance_name.set_text("1.21.4-Fabric");

    for (i, name) in pages.iter().enumerate() {
        controller.goto(i);
        ui.refresh().unwrap();
        if let Some(out) = &out {
            render(&ui, light_theme(), &out.join(format!("{i}-{name}-light.png")));
        }
    }

    // Each settings category beside the navigation column.
    controller.goto(5);
    for (page, name) in ["game", "java", "download", "appearance"].iter().enumerate().skip(1) {
        shell.settings_page.set(page as i64).unwrap();
        ui.refresh().unwrap();
        if let Some(out) = &out {
            render(&ui, light_theme(), &out.join(format!("5-settings-{name}-light.png")));
        }
    }
    shell.settings_page.set(0).unwrap();

    // The sign-in dialog once the device code has arrived.
    controller.goto(1);
    shell.login_status.set("已在浏览器中打开微软登录页，代码已自动填入；如果没有打开，请手动访问下面的网址并输入代码。登录完成后这里会自动继续。".into()).unwrap();
    shell.login_url.set("https://www.microsoft.com/link?otc=ABCD1234".into()).unwrap();
    shell.login_code.set("ABCD1234".into()).unwrap();
    shell.login_open.set(true).unwrap();
    ui.refresh().unwrap();
    if let Some(out) = &out {
        render(&ui, light_theme(), &out.join("1-login-light.png"));
    }
    drop((ui, controller));

    let (ui, controller) = build(dark_theme(), dir.path());
    controller.apply_theme(ThemePreference::Dark);
    for (i, name) in pages.iter().enumerate() {
        controller.goto(i);
        ui.refresh().unwrap();
        if let Some(out) = &out {
            render(&ui, dark_theme(), &out.join(format!("{i}-{name}-dark.png")));
        }
    }
}
