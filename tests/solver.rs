//! Runs the solver on the GPU. Skipped without the demo files in data/.
use box_of_smoke::data::Files;
use box_of_smoke::sim::{ATLAS, COLS, DIM};
use box_of_smoke::{App, gpu};

fn app() -> Option<App> {
    let Ok(files) = Files::from_dir(std::path::Path::new("data")) else {
        eprintln!("skipped: no demo files in data/");
        return None;
    };
    Some(App::new(gpu::Gpu::headless().unwrap(), &files, gpu::COLOR).unwrap())
}

fn run(app: &mut App, steps: usize) {
    for _ in 0..steps {
        let mut enc = app.gpu.device.create_command_encoder(&Default::default());
        app.sim.begin();
        app.step(&mut enc);
        app.gpu.queue.submit([enc.finish()]);
    }
}

fn half(b: u16) -> f32 {
    let (sign, exp, man) = (b >> 15, (b >> 10) & 31, f32::from(b & 1023));
    let v = match exp {
        0 => man * 2f32.powi(-24),
        31 if man == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        e => (1.0 + man / 1024.0) * 2f32.powi(i32::from(e) - 15),
    };
    if sign == 1 { -v } else { v }
}

/// Values by cell.
struct Field(Vec<[f32; 4]>);

impl Field {
    fn read(app: &App, t: &wgpu::Texture) -> Self {
        let bytes = app.gpu.read(t);
        let values: Vec<f32> = bytes
            .chunks_exact(2)
            .map(|b| half(u16::from_le_bytes([b[0], b[1]])))
            .collect();
        assert!(values.iter().all(|v| v.is_finite()), "not finite");
        Self(
            values
                .chunks_exact(4)
                .map(|c| [c[0], c[1], c[2], c[3]])
                .collect(),
        )
    }
    fn at(&self, x: i32, y: i32, z: i32) -> [f32; 4] {
        let px = x + DIM.x * (z % COLS);
        let py = y + DIM.y * (z / COLS);
        self.0[(py * ATLAS.0 as i32 + px) as usize]
    }
    fn cells(margin: i32) -> impl Iterator<Item = (i32, i32, i32)> {
        let m = margin;
        (m..DIM.z - m).flat_map(move |z| {
            (m..DIM.y - m).flat_map(move |y| (m..DIM.x - m).map(move |x| (x, y, z)))
        })
    }
    fn sum(&self, c: usize) -> f64 {
        Self::cells(1)
            .map(|(x, y, z)| f64::from(self.at(x, y, z)[c]))
            .sum()
    }
}

#[test]
fn smoke_stays_finite_and_inside() {
    let Some(mut app) = app() else { return };
    run(&mut app, 600);
    let d = Field::read(&app, app.sim.density_texture());
    let total = d.sum(3);
    assert!(total > 100.0 && total < 100_000.0, "density {total}");
    let v = Field::read(&app, app.sim.velocity_texture());
    let fastest = Field::cells(1).map(|(x, y, z)| v.at(x, y, z)[..3].iter().map(|c| c * c).sum());
    let fastest = fastest.fold(0f32, f32::max).sqrt();
    assert!(fastest > 0.1 && fastest < 40.0, "speed {fastest}");
}

#[test]
fn smoke_fades_at_the_set_rate() {
    let Some(mut app) = app() else { return };
    // Still air: the logo mode has no source until it has been idle.
    app.controls.key("n");
    (0..15).for_each(|_| assert!(app.controls.key("{")));
    run(&mut app, 1);
    let before = Field::read(&app, app.sim.density_texture()).sum(3);
    run(&mut app, 60);
    let after = Field::read(&app, app.sim.density_texture()).sum(3);
    let rate = (after / before).powf(1.0 / 60.0);
    assert!((rate - 0.985).abs() < 0.002, "kept {rate} per step");
}

/// Mean absolute divergence, in the solver's own differences, over cells away from
/// the walls and more than `away` cells from the source.
fn divergence(app: &App, away: f32) -> f32 {
    let v = Field::read(app, app.sim.velocity_texture());
    let (mut sum, mut n) = (0.0, 0.0);
    for (x, y, z) in Field::cells(4) {
        let here = glam::Vec3::new(x as f32, y as f32, z as f32);
        if here.distance(app.controls.inject.pos) < away {
            continue;
        }
        let d = v.at(x + 1, y, z)[0] - v.at(x - 1, y, z)[0] + v.at(x, y + 1, z)[1]
            - v.at(x, y - 1, z)[1]
            + v.at(x, y, z + 1)[2]
            - v.at(x, y, z - 1)[2];
        sum += (0.5 * d).abs();
        n += 1.0;
    }
    sum / n
}

#[test]
fn projection_removes_divergence() {
    let Some(mut app) = app() else { return };
    run(&mut app, 120);
    // Let the pressure settle with no new forces.
    app.controls.settings.confinement = 0.0;
    let mut seen = vec![divergence(&app, 0.0)];
    for _ in 0..3 {
        for _ in 0..20 {
            app.controls
                .press(box_of_smoke::controls::Button::Left, false, (0.5, 0.5));
            app.controls.release();
            app.controls.inject.active = false;
            run(&mut app, 1);
        }
        seen.push(divergence(&app, 0.0));
    }
    eprintln!("mean divergence {seen:?}");
    assert!(seen[3] < seen[0] / 2.0, "{seen:?}");
}

#[test]
fn logo_keeps_its_smoke() {
    let Some(mut app) = app() else { return };
    app.controls.key("n");
    run(&mut app, 1);
    let before = Field::read(&app, app.sim.density_texture()).sum(3);
    run(&mut app, 100);
    let after = Field::read(&app, app.sim.density_texture()).sum(3);
    assert!(before > 1000.0);
    assert!((after / before - 1.0).abs() < 0.02, "{before} to {after}");
}

#[test]
fn water_settles_level() {
    let Some(mut app) = app() else { return };
    app.controls.key("w");
    run(&mut app, 400);
    let d = Field::read(&app, app.sim.density_texture());
    // Below the surface the level set is negative, above it positive.
    let (mut low, mut high, mut n) = (0, 0, 0);
    for z in 8..DIM.z - 8 {
        for x in 8..DIM.x - 8 {
            n += 1;
            low += i32::from(d.at(x, 12, z)[3] < 0.0);
            high += i32::from(d.at(x, 40, z)[3] > 0.0);
        }
    }
    assert!(
        low * 10 > n * 9 && high * 10 > n * 9,
        "{low} and {high} of {n}"
    );
}
