//! Particle spawners (`particle_spawner_update`, `particle_spawner_spawn`).
//!
//! A spawner is simulated in steps of a sixtieth of a second. Each step it
//! may spawn particles (a steady stream spread over a minute, or bursts
//! when fired) and moves the ones it has. The simulation only runs
//! forwards: the editor steps it along with the animation and clears it
//! when the animation jumps back, as the original does.
//!
//! Not simulated yet: spawning along and being held to paths, and being
//! pulled along a path.

mod rng;

pub use rng::Rng;

use mi_core::Color;
use mi_format::project::{ParticleSource, ParticleSpawner, ParticleTypeSettings, SpawnerSettings};

/// Steps per second.
pub const STEPS_PER_SECOND: f64 = 60.0;

/// Steps the steady stream of a spawner is spread over (`minute_steps`).
const QUEUE_STEPS: usize = 60 * 60;

type Vec3 = [f64; 3];

/// One particle.
#[derive(Debug, Clone, PartialEq)]
pub struct Particle {
    /// Index of its type in the spawner.
    pub kind: usize,
    pub spawn_step: i64,
    /// Sprite frame, or image of the template.
    pub frame: i64,
    time: f64,
    freeze_time: f64,
    time_to_live: f64,
    pub pos: Vec3,
    /// Direction it is launched in, and how fast it moves along it.
    angle: Vec3,
    angle_speed: f64,
    angle_speed_add: f64,
    angle_speed_mul: f64,
    spd: Vec3,
    spd_add: Vec3,
    spd_mul: Vec3,
    /// Rotation of particles that are objects.
    pub rot: Vec3,
    rot_spd: Vec3,
    rot_spd_add: Vec3,
    rot_spd_mul: Vec3,
    /// Turn of a sprite around the line of sight, in degrees.
    pub sprite_angle: f64,
    sprite_angle_add: f64,
    pub scale: f64,
    scale_add: f64,
    pub alpha: f64,
    alpha_add: f64,
    pub color: Color,
    /// Start colour, target colour and steps the change takes.
    color_mix: Option<(Color, Color, f64)>,
    animation_speed: f64,
}

/// What a spawner timeline is doing at a step.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnerState {
    pub world_pos: Vec3,
    /// Turns launch directions into the world: the linear part of the
    /// timeline's matrix, rows as in the original.
    pub rotation: [Vec3; 3],
    /// Spawning (the `SPAWN` value).
    pub spawn: bool,
    /// Particles stand still (the `FREEZE` value).
    pub freeze: bool,
    /// The seed, when the timeline has one of its own.
    pub seed: Option<f64>,
    /// Where particles are pulled to, and how hard.
    pub attractor: Option<Vec3>,
    pub force: f64,
}

impl Default for SpawnerState {
    fn default() -> Self {
        Self {
            world_pos: [0.0; 3],
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            spawn: true,
            freeze: false,
            seed: None,
            attractor: None,
            force: 1.0,
        }
    }
}

/// The particles of one spawner.
#[derive(Debug, Clone, Default)]
pub struct Spawner {
    pub particles: Vec<Particle>,
    /// The step the simulation has been run up to.
    last_step: Option<i64>,
    /// Types to spawn at each step of the current minute.
    queue: Vec<Vec<usize>>,
    queue_start: Option<i64>,
    /// A burst is due.
    fire: bool,
    /// Spawns without a seed of their own are numbered, to tell them apart.
    spawned: u64,
}

fn merge_color(a: Color, b: Color, amount: f64) -> Color {
    let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * amount).round().clamp(0.0, 255.0) as u8;
    Color::rgb(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}

fn value_random(rng: &mut Rng, value: f64, is_random: bool, min: f64, max: f64) -> f64 {
    if is_random {
        rng.range(min, max)
    } else {
        value
    }
}

/// A multiplier per second as one per step (the original takes the square
/// root five times, which is the 32nd root).
fn per_step_factor(factor: f64) -> f64 {
    if factor == 1.0 {
        return 1.0;
    }
    (0..5).fold(factor, |f, _| f.sqrt())
}

fn normalize(v: Vec3) -> Vec3 {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length > 0.0 {
        v.map(|c| c / length)
    } else {
        v
    }
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `vec3_reflect`
fn reflect(v: Vec3, normal: Vec3) -> Vec3 {
    let d = 2.0 * dot(v, normal);
    [v[0] - d * normal[0], v[1] - d * normal[1], v[2] - d * normal[2]]
}

fn lengthdir_x(length: f64, degrees: f64) -> f64 {
    length * degrees.to_radians().cos()
}

fn lengthdir_y(length: f64, degrees: f64) -> f64 {
    -length * degrees.to_radians().sin()
}

/// The launch direction for angles in degrees: +Z turned by them
/// (`matrix_create(0, angle, 1)` applied to (0, 0, 1)).
fn launch_direction(angle: Vec3) -> Vec3 {
    normalize(mi_anim::Mat4::build([0.0; 3], angle, [1.0; 3]).transform_vector([0.0, 0.0, 1.0]))
}

/// `particle_get_animation_percent`
fn animation_percent(step: i64, start_time: f64, start_frame: f64, end_frame: f64, speed: f64, on_end: f64) -> f64 {
    if start_frame == end_frame {
        // The original returns the frame here, which callers treat as a
        // percentage like any other.
        return start_frame;
    }
    let seconds = (step as f64 - start_time) / STEPS_PER_SECOND;
    let percent = seconds * speed / (start_frame - end_frame).abs();
    match on_end as i64 {
        0 => percent.min(1.0),
        1 => percent % 1.0,
        2 => {
            if (percent % 2.0).floor() != 0.0 {
                1.0 - percent % 1.0
            } else {
                percent % 1.0
            }
        }
        _ => percent,
    }
}

/// Per component, or the first component for all when a setting is not
/// "extended" to separate axes.
fn component(values: Vec3, extend: bool, axis: usize) -> f64 {
    values[if extend { axis } else { 0 }]
}

fn flag(values: [bool; 3], extend: bool, axis: usize) -> bool {
    values[if extend { axis } else { 0 }]
}

impl Spawner {
    /// Removes all particles (`particle_spawner_clear`).
    pub fn clear(&mut self) {
        self.particles.clear();
        self.queue_start = None;
    }

    /// Asks for a burst at the next step (spawners that do not spawn
    /// steadily).
    pub fn fire(&mut self) {
        self.fire = true;
    }

    /// The step the spawner has been simulated up to.
    pub fn step(&self) -> Option<i64> {
        self.last_step
    }

    /// Runs the simulation up to `step`. Going back in time clears the
    /// particles; the first call only notes where time starts.
    /// `template_frames` gives the number of images of a particle
    /// template by name.
    pub fn advance(&mut self, step: i64, spawner: &ParticleSpawner, state: &SpawnerState, template_frames: &dyn Fn(&str) -> i64) {
        let last = match self.last_step {
            Some(last) if step < last => {
                self.clear();
                step
            }
            Some(last) => last,
            None => step,
        };
        for s in last..step {
            self.spawn_step(s, spawner, state, template_frames);
            self.update_step(s, spawner, state, template_frames);
        }
        self.last_step = Some(step);
    }

    fn spawn_step(&mut self, s: i64, spawner: &ParticleSpawner, state: &SpawnerState, template_frames: &dyn Fn(&str) -> i64) {
        let settings = &spawner.settings;
        let per_type = |rate: f64| (rate * settings.spawn_amount).floor().max(0.0) as usize;
        if settings.spawn_constant {
            if !state.spawn {
                return;
            }
            // A new minute: spread every type's particles over it.
            if self.queue_start.is_none_or(|start| s >= start + QUEUE_STEPS as i64) {
                self.queue = vec![Vec::new(); QUEUE_STEPS];
                let mut rng = Rng::new(state.seed.unwrap_or(0.0) + s as f64);
                for (index, kind) in spawner.types.iter().enumerate() {
                    for _ in 0..per_type(kind.settings.spawn_rate) {
                        let slot = rng.irandom(QUEUE_STEPS as i64 - 1) as usize;
                        self.queue[slot].push(index);
                    }
                }
                self.queue_start = Some(s);
            }
            let slot = (s - self.queue_start.unwrap_or(s)) as usize;
            for kind in self.queue.get(slot).cloned().unwrap_or_default() {
                let count = self.particles.len() as f64;
                self.spawn(s, kind, count, spawner, state, template_frames);
            }
        } else if self.fire {
            self.fire = false;
            for (index, kind) in spawner.types.iter().enumerate() {
                for fired in 1..=per_type(kind.settings.spawn_rate) {
                    self.spawn(s, index, fired as f64, spawner, state, template_frames);
                }
            }
        }
    }

    /// `particle_spawner_spawn`. `number` tells spawns with the same seed
    /// apart: the number of particles there are, or the count within a
    /// burst.
    fn spawn(
        &mut self,
        step: i64,
        kind: usize,
        number: f64,
        spawner: &ParticleSpawner,
        state: &SpawnerState,
        template_frames: &dyn Fn(&str) -> i64,
    ) {
        let settings = &spawner.settings;
        let t = &spawner.types[kind].settings;
        self.spawned += 1;
        // The original seeds spawners without a seed of their own from the
        // clock; here they get a sequence that repeats when replayed.
        let seed = match state.seed {
            Some(seed) => number + seed,
            None => (step as f64 * 7919.0 + self.spawned as f64 * 104_729.0) % 4_294_967_296.0,
        };
        let mut rng = Rng::new(seed);

        let mut frame = 0;
        let is_template = matches!(spawner.types[kind].source, ParticleSource::Template);
        if t.sprite_template_still_frame && t.sprite_template_random_frame && is_template {
            frame = rng.random(template_frames(&t.sprite_template) as f64) as i64;
        }
        let time_to_live = value_random(
            &mut rng,
            settings.destroy_at_time_seconds,
            settings.destroy_at_time_israndom,
            settings.destroy_at_time_random_min,
            settings.destroy_at_time_random_max,
        ) * STEPS_PER_SECOND;

        let mut pos = state.world_pos;
        if settings.spawn_region_use && t.spawn_region {
            match settings.spawn_region_type.as_str() {
                "sphere" => {
                    let xy = rng.random(360.0);
                    let z = rng.range(-180.0, 180.0);
                    let radius = settings.spawn_region_sphere_radius;
                    let distance = rng.range(-radius, radius);
                    pos[0] += lengthdir_x(distance, xy) * lengthdir_x(1.0, z);
                    pos[1] += lengthdir_y(distance, xy) * lengthdir_x(1.0, z);
                    pos[2] += -lengthdir_y(distance, z);
                }
                "cube" => {
                    let half = settings.spawn_region_cube_size / 2.0;
                    for p in &mut pos {
                        *p += rng.range(-half, half);
                    }
                }
                "box" => {
                    for (p, size) in pos.iter_mut().zip(settings.spawn_region_box_size) {
                        *p += rng.range(-size / 2.0, size / 2.0);
                    }
                }
                // Paths: not yet; particles start at the spawner.
                _ => {}
            }
        }

        let animation_speed = value_random(
            &mut rng,
            t.sprite_animation_speed,
            t.sprite_animation_speed_israndom,
            t.sprite_animation_speed_random_min,
            t.sprite_animation_speed_random_max,
        );

        let (mut angle, mut spd, mut spd_add, mut spd_mul) = ([0.0; 3], [0.0; 3], [0.0; 3], [1.0; 3]);
        let (mut rot, mut rot_spd, mut rot_spd_add, mut rot_spd_mul) = ([0.0; 3], [0.0; 3], [0.0; 3], [1.0; 3]);
        for a in 0..3 {
            let pick = |rng: &mut Rng, value: Vec3, random: [bool; 3], min: Vec3, max: Vec3, extend: bool| {
                value_random(rng, component(value, extend, a), flag(random, extend, a), component(min, extend, a), component(max, extend, a))
            };
            angle[a] = pick(&mut rng, t.angle, t.angle_israndom, t.angle_random_min, t.angle_random_max, t.angle_extend);
            spd[a] = pick(&mut rng, t.spd, t.spd_israndom, t.spd_random_min, t.spd_random_max, t.spd_extend) / STEPS_PER_SECOND;
            spd_add[a] =
                pick(&mut rng, t.spd_add, t.spd_add_israndom, t.spd_add_random_min, t.spd_add_random_max, t.spd_extend) / STEPS_PER_SECOND;
            spd_mul[a] =
                per_step_factor(pick(&mut rng, t.spd_mul, t.spd_mul_israndom, t.spd_mul_random_min, t.spd_mul_random_max, t.spd_extend));
            rot[a] = pick(&mut rng, t.rot, t.rot_israndom, t.rot_random_min, t.rot_random_max, t.rot_extend);
            rot_spd[a] = pick(&mut rng, t.rot_spd, t.rot_spd_israndom, t.rot_spd_random_min, t.rot_spd_random_max, t.rot_spd_extend)
                / STEPS_PER_SECOND;
            rot_spd_add[a] = pick(
                &mut rng,
                t.rot_spd_add,
                t.rot_spd_add_israndom,
                t.rot_spd_add_random_min,
                t.rot_spd_add_random_max,
                t.rot_spd_extend,
            ) / STEPS_PER_SECOND;
            rot_spd_mul[a] = per_step_factor(pick(
                &mut rng,
                t.rot_spd_mul,
                t.rot_spd_mul_israndom,
                t.rot_spd_mul_random_min,
                t.rot_spd_mul_random_max,
                t.rot_spd_extend,
            ));
        }

        // The launch direction, turned with the spawner.
        let local = launch_direction(angle);
        let r = state.rotation;
        let direction = normalize([
            local[0] * r[0][0] + local[1] * r[1][0] + local[2] * r[2][0],
            local[0] * r[0][1] + local[1] * r[1][1] + local[2] * r[2][1],
            local[0] * r[0][2] + local[1] * r[1][2] + local[2] * r[2][2],
        ]);

        let angle_speed =
            value_random(&mut rng, t.angle_speed, t.angle_speed_israndom, t.angle_speed_random_min, t.angle_speed_random_max)
                / STEPS_PER_SECOND;
        let angle_speed_add = value_random(
            &mut rng,
            t.angle_speed_add,
            t.angle_speed_add_israndom,
            t.angle_speed_add_random_min,
            t.angle_speed_add_random_max,
        ) / STEPS_PER_SECOND;
        let angle_speed_mul = per_step_factor(value_random(
            &mut rng,
            t.angle_speed_mul,
            t.angle_speed_mul_israndom,
            t.angle_speed_mul_random_min,
            t.angle_speed_mul_random_max,
        ));

        let sprite_angle =
            value_random(&mut rng, t.sprite_angle, t.sprite_angle_israndom, t.sprite_angle_random_min, t.sprite_angle_random_max);
        let sprite_angle_add = value_random(
            &mut rng,
            t.sprite_angle_add,
            t.sprite_angle_add_israndom,
            t.sprite_angle_add_random_min,
            t.sprite_angle_add_random_max,
        ) / STEPS_PER_SECOND;
        let scale = value_random(&mut rng, t.scale, t.scale_israndom, t.scale_random_min, t.scale_random_max);
        let scale_add = value_random(&mut rng, t.scale_add, t.scale_add_israndom, t.scale_add_random_min, t.scale_add_random_max)
            / STEPS_PER_SECOND;
        let alpha = value_random(&mut rng, t.alpha, t.alpha_israndom, t.alpha_random_min, t.alpha_random_max);
        let alpha_add = value_random(&mut rng, t.alpha_add, t.alpha_add_israndom, t.alpha_add_random_min, t.alpha_add_random_max)
            / STEPS_PER_SECOND;

        let color =
            if t.color_israndom { merge_color(t.color_random_start, t.color_random_end, rng.random(1.0)) } else { t.color };
        let color_mix = t.color_mix_enabled.then(|| {
            let target = if t.color_mix_israndom {
                merge_color(t.color_mix_random_start, t.color_mix_random_end, rng.random(1.0))
            } else {
                t.color_mix
            };
            let seconds = value_random(
                &mut rng,
                t.color_mix_time,
                t.color_mix_time_israndom,
                t.color_mix_time_random_min,
                t.color_mix_time_random_max,
            );
            (color, target, (seconds * STEPS_PER_SECOND).max(1.0))
        });

        self.particles.push(Particle {
            kind,
            spawn_step: step,
            frame,
            time: 0.0,
            freeze_time: 0.0,
            time_to_live,
            pos,
            angle: direction,
            angle_speed,
            angle_speed_add,
            angle_speed_mul,
            spd,
            spd_add,
            spd_mul,
            rot,
            rot_spd,
            rot_spd_add,
            rot_spd_mul,
            sprite_angle,
            sprite_angle_add,
            scale,
            scale_add,
            alpha,
            alpha_add,
            color,
            color_mix,
            animation_speed,
        });
    }

    /// Moves every particle by one step; removes those that are done.
    fn update_step(&mut self, s: i64, spawner: &ParticleSpawner, state: &SpawnerState, template_frames: &dyn Fn(&str) -> i64) {
        let settings = &spawner.settings;
        let mut index = 0;
        while index < self.particles.len() {
            // Too many: the oldest go first.
            if settings.destroy_at_amount && self.particles.len() as f64 > settings.destroy_at_amount_val {
                self.particles.remove(index);
                continue;
            }
            if state.freeze {
                self.particles[index].freeze_time += 1.0;
                index += 1;
                continue;
            }
            let kind = &spawner.types[self.particles[index].kind];
            let keep = update_particle(&mut self.particles[index], s, settings, &kind.settings, &kind.source, state, template_frames);
            if keep {
                index += 1;
            } else {
                self.particles.remove(index);
            }
        }
    }
}

/// One step of one particle. Returns whether it lives on.
fn update_particle(
    pt: &mut Particle,
    s: i64,
    settings: &SpawnerSettings,
    t: &ParticleTypeSettings,
    source: &ParticleSource,
    state: &SpawnerState,
    template_frames: &dyn Fn(&str) -> i64,
) -> bool {
    pt.time += 1.0;
    if settings.destroy_at_time && pt.time >= pt.time_to_live {
        return false;
    }

    // Animation frame. The step counted here is the one being reached.
    let now = s + 1;
    let started = pt.spawn_step as f64 + pt.freeze_time;
    match source {
        ParticleSource::Sheet => {
            let ani = animation_percent(now, started, t.sprite_frame_start, t.sprite_frame_end, pt.animation_speed, t.sprite_animation_onend);
            if ani == 1.0 && settings.destroy_at_animation_finish && t.sprite_animation_onend == 0.0 {
                return false;
            }
            pt.frame = (t.sprite_frame_start + (t.sprite_frame_end - t.sprite_frame_start) * ani).round() as i64;
        }
        ParticleSource::Template if !t.sprite_template_still_frame => {
            let frames = template_frames(&t.sprite_template).max(1) as f64;
            let (start, end) = if t.sprite_template_reverse { (frames - 1.0, 0.0) } else { (0.0, frames - 1.0) };
            let ani = animation_percent(now, started, start, end, pt.animation_speed, t.sprite_animation_onend);
            if ani == 1.0 && settings.destroy_at_animation_finish && t.sprite_animation_onend == 0.0 {
                return false;
            }
            pt.frame = (start + (end - start) * ani).round() as i64;
        }
        _ => {}
    }

    // Launch direction
    for a in 0..3 {
        pt.pos[a] += pt.angle[a] * pt.angle_speed;
    }
    pt.angle_speed += pt.angle_speed_add;
    pt.angle_speed *= pt.angle_speed_mul;

    // Speed, rotation and attraction
    for a in 0..3 {
        pt.pos[a] += pt.spd[a];
        pt.spd[a] += pt.spd_add[a];
        pt.spd[a] *= pt.spd_mul[a];
        pt.rot[a] += pt.rot_spd[a];
        pt.rot_spd[a] += pt.rot_spd_add[a];
        pt.rot_spd[a] *= pt.rot_spd_mul[a];
        if let Some(target) = state.attractor {
            // Orbiting particles are pulled from where they are, the
            // others as if they were at the spawner.
            let from = if t.orbit { pt.pos[a] } else { state.world_pos[a] };
            pt.spd[a] += (target[a] - from).clamp(-state.force, state.force) / STEPS_PER_SECOND;
        }
    }

    pt.sprite_angle += pt.sprite_angle_add;
    pt.scale += pt.scale_add;
    if pt.scale <= 0.0 {
        return false;
    }
    pt.alpha += pt.alpha_add;
    if pt.alpha <= 0.0 {
        return false;
    }
    if let Some((start, target, steps)) = pt.color_mix {
        pt.color = merge_color(start, target, ((s - pt.spawn_step) as f64 / steps).clamp(0.0, 1.0));
    }

    // Bounding box
    let mut hit = false;
    if settings.bounding_box_type != "none" && t.bounding_box {
        let mut bounds: Option<(Vec3, Vec3)> = None;
        match settings.bounding_box_type.as_str() {
            "ground" => {
                let ground = settings.bounding_box_ground_z;
                if pt.pos[2] < ground {
                    hit = true;
                    if t.bounce {
                        pt.spd[2] *= -t.bounce_factor;
                        if pt.spd[2].abs() < 0.25 {
                            pt.spd[2] = 0.0;
                        }
                        for spin in &mut pt.rot_spd {
                            *spin *= -0.5;
                        }
                        // Friction along the ground. (The original slows X
                        // twice and Y not at all.)
                        pt.spd_mul[0] *= 0.995;
                        pt.spd_mul[1] *= 0.995;
                        pt.angle = reflect(pt.angle, [0.0, 0.0, 1.0]);
                        pt.angle_speed *= t.bounce_factor;
                    }
                    pt.pos[2] = pt.pos[2].max(ground);
                }
            }
            "spawn" => {
                if !settings.spawn_region_use {
                    return true;
                }
                match settings.spawn_region_type.as_str() {
                    "sphere" => {
                        let offset = [pt.pos[0] - state.world_pos[0], pt.pos[1] - state.world_pos[1], pt.pos[2] - state.world_pos[2]];
                        let distance = dot(offset, offset).sqrt();
                        let radius = settings.spawn_region_sphere_radius;
                        if distance > radius {
                            hit = true;
                            if t.bounce {
                                for a in 0..3 {
                                    pt.spd[a] *= -t.bounce_factor;
                                    if pt.spd[a].abs() < 0.1 {
                                        pt.spd[a] = 0.0;
                                    }
                                    pt.rot_spd[a] *= -0.5;
                                }
                            }
                            let outward = normalize(offset);
                            pt.angle = reflect(pt.angle, outward);
                            pt.angle_speed *= t.bounce_factor;
                            pt.pos = [0, 1, 2].map(|a| state.world_pos[a] + outward[a] * radius);
                        }
                    }
                    "cube" => {
                        let half = settings.spawn_region_cube_size / 2.0;
                        bounds = Some((state.world_pos.map(|c| c - half), state.world_pos.map(|c| c + half)));
                    }
                    "box" => {
                        let size = settings.spawn_region_box_size;
                        bounds = Some((
                            [0, 1, 2].map(|a| state.world_pos[a] - size[a] / 2.0),
                            [0, 1, 2].map(|a| state.world_pos[a] + size[a] / 2.0),
                        ));
                    }
                    _ => {}
                }
            }
            "custom" => {
                let relative = if settings.bounding_box_relative { 1.0 } else { 0.0 };
                bounds = Some((
                    [0, 1, 2].map(|a| state.world_pos[a] * relative + settings.bounding_box_custom_start[a]),
                    [0, 1, 2].map(|a| state.world_pos[a] * relative + settings.bounding_box_custom_end[a]),
                ));
            }
            _ => {}
        }

        if let Some((start, end)) = bounds {
            // The face it left through; the last one found counts.
            let mut normal = None;
            for a in 0..3 {
                let mut n = [0.0; 3];
                if pt.pos[a] < start[a] {
                    n[a] = 1.0;
                    normal = Some(n);
                }
                if pt.pos[a] > end[a] {
                    n[a] = -1.0;
                    normal = Some(n);
                }
            }
            // The original clamps the position while reflecting the launch
            // direction, which hides the exit from its own per-axis bounce
            // of the speed; here both happen.
            let outside: [bool; 3] = [0, 1, 2].map(|a| pt.pos[a] < start[a] || pt.pos[a] > end[a]);
            if let Some(normal) = normal {
                hit = true;
                pt.angle = reflect(pt.angle, normal);
                pt.angle_speed *= t.bounce_factor;
            }
            for a in 0..3 {
                if outside[a] {
                    pt.pos[a] = pt.pos[a].clamp(start[a], end[a]);
                    if t.bounce {
                        pt.spd[a] *= -t.bounce_factor;
                        if pt.spd[a].abs() < 0.25 {
                            pt.spd[a] = 0.0;
                        }
                        for spin in &mut pt.rot_spd {
                            *spin *= -0.5;
                        }
                    }
                }
            }
        }
    }
    !(hit && settings.destroy_at_bounding_box)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::SaveId;
    use mi_format::project::ParticleType;

    fn frames(_: &str) -> i64 {
        8
    }

    /// A spawner with one type that does nothing by itself.
    fn spawner(edit: impl FnOnce(&mut SpawnerSettings, &mut ParticleTypeSettings)) -> ParticleSpawner {
        let mut spawner = ParticleSpawner::default();
        let mut kind = ParticleType::new(SaveId::new("TYPE"));
        kind.source = ParticleSource::Template;
        kind.settings.spawn_rate = 1.0;
        kind.settings.angle_israndom = [false; 3];
        kind.settings.angle_speed = 0.0;
        kind.settings.angle_speed_israndom = false;
        kind.settings.rot_israndom = [false; 3];
        kind.settings.rot_spd_israndom = [false; 3];
        kind.settings.sprite_template_still_frame = true;
        spawner.settings.destroy_at_amount = false;
        edit(&mut spawner.settings, &mut kind.settings);
        spawner.types.push(kind);
        spawner
    }

    fn seeded() -> SpawnerState {
        SpawnerState { seed: Some(5.0), ..Default::default() }
    }

    #[test]
    fn a_steady_spawner_spreads_its_amount_over_a_minute() {
        let config = spawner(|s, _| s.spawn_amount = 600.0);
        let mut sim = Spawner::default();
        // The first call starts the clock.
        sim.advance(0, &config, &seeded(), &frames);
        assert!(sim.particles.is_empty());
        sim.advance(QUEUE_STEPS as i64, &config, &seeded(), &frames);
        assert_eq!(sim.particles.len(), 600);
        // About a sixth after ten seconds.
        let mut partial = Spawner::default();
        partial.advance(0, &config, &seeded(), &frames);
        partial.advance(600, &config, &seeded(), &frames);
        assert!((60..140).contains(&partial.particles.len()), "{}", partial.particles.len());
        // Not spawning: nothing new.
        let mut off = Spawner::default();
        let idle = SpawnerState { spawn: false, ..seeded() };
        off.advance(0, &config, &idle, &frames);
        off.advance(600, &config, &idle, &frames);
        assert!(off.particles.is_empty());
    }

    #[test]
    fn the_same_seed_gives_the_same_particles() {
        let config = spawner(|s, t| {
            s.spawn_amount = 300.0;
            t.spd_israndom = [true; 3];
            t.spd_extend = true;
            t.scale_israndom = true;
            t.color_israndom = true;
        });
        let run = |state: &SpawnerState| {
            let mut sim = Spawner::default();
            sim.advance(0, &config, state, &frames);
            // In two parts or at once makes no difference.
            sim.advance(400, &config, state, &frames);
            sim.advance(900, &config, state, &frames);
            sim.particles
        };
        let a = run(&seeded());
        assert!(!a.is_empty());
        assert_eq!(a, run(&seeded()));
        assert_ne!(a, run(&SpawnerState { seed: Some(6.0), ..Default::default() }));
        // Without a seed of its own a spawner still repeats when replayed.
        assert_eq!(run(&SpawnerState::default()), run(&SpawnerState::default()));
        let mut at_once = Spawner::default();
        at_once.advance(0, &config, &seeded(), &frames);
        at_once.advance(900, &config, &seeded(), &frames);
        assert_eq!(at_once.particles, a);
    }

    #[test]
    fn bursts_fire_once_and_particles_move_and_fade() {
        let config = spawner(|s, t| {
            s.spawn_constant = false;
            s.spawn_amount = 10.0;
            t.spd = [60.0, 0.0, 120.0];
            t.spd_extend = true;
            t.spd_add = [0.0, 0.0, -1.2];
            t.alpha_add = -0.5;
            t.scale_add = 1.0;
        });
        let state = SpawnerState { world_pos: [10.0, 20.0, 30.0], ..seeded() };
        let mut sim = Spawner::default();
        sim.advance(0, &config, &state, &frames);
        sim.advance(30, &config, &state, &frames);
        assert!(sim.particles.is_empty());
        sim.fire();
        sim.advance(31, &config, &state, &frames);
        assert_eq!(sim.particles.len(), 10);
        // One step: a sixtieth of the speed per second.
        let first = &sim.particles[0];
        assert!((first.pos[0] - 11.0).abs() < 1e-9 && (first.pos[2] - 32.0).abs() < 1e-9, "{:?}", first.pos);
        // After a second: moved 60 along X, faded by half, grown by one,
        // and gravity has slowed the rise (2 per step, less 0.02 a step).
        sim.advance(90, &config, &state, &frames);
        assert_eq!(sim.particles.len(), 10);
        let p = &sim.particles[0];
        assert!((p.pos[0] - 70.0).abs() < 1e-6);
        assert!((p.alpha - 0.5).abs() < 1e-9 && (p.scale - 2.0).abs() < 1e-9);
        let rise: f64 = (0..60).map(|i| 2.0 - i as f64 * 0.02).sum();
        assert!((p.pos[2] - (30.0 + rise)).abs() < 1e-6, "{}", p.pos[2]);
        // After two seconds they have faded away.
        sim.advance(151, &config, &state, &frames);
        assert!(sim.particles.is_empty());
        // It does not fire again by itself.
        sim.advance(400, &config, &state, &frames);
        assert!(sim.particles.is_empty());
    }

    #[test]
    fn limits_remove_particles() {
        // By lifetime.
        let timed = spawner(|s, _| {
            s.spawn_constant = false;
            s.spawn_amount = 4.0;
            s.destroy_at_time = true;
            s.destroy_at_time_seconds = 0.5;
        });
        let mut sim = Spawner::default();
        sim.advance(0, &timed, &seeded(), &frames);
        sim.fire();
        sim.advance(29, &timed, &seeded(), &frames);
        assert_eq!(sim.particles.len(), 4);
        sim.advance(31, &timed, &seeded(), &frames);
        assert!(sim.particles.is_empty());

        // By number: the oldest go.
        let capped = spawner(|s, _| {
            s.spawn_amount = 3600.0;
            s.destroy_at_amount = true;
            s.destroy_at_amount_val = 50.0;
        });
        let mut sim = Spawner::default();
        sim.advance(0, &capped, &seeded(), &frames);
        sim.advance(600, &capped, &seeded(), &frames);
        assert!(sim.particles.len() <= 51, "{}", sim.particles.len());
        assert!(sim.particles.iter().all(|p| p.spawn_step > 500));

        // Frozen particles stay as they are; going back clears them.
        let frozen = SpawnerState { freeze: true, ..seeded() };
        let before = sim.particles.clone();
        sim.advance(660, &capped, &SpawnerState { spawn: false, ..frozen.clone() }, &frames);
        assert_eq!(sim.particles.len(), before.len());
        assert_eq!(sim.particles[0].pos, before[0].pos);
        sim.advance(100, &capped, &seeded(), &frames);
        assert!(sim.particles.is_empty());
        assert_eq!(sim.step(), Some(100));
    }

    #[test]
    fn the_ground_stops_and_bounces_particles() {
        let config = spawner(|s, t| {
            s.spawn_constant = false;
            s.spawn_amount = 1.0;
            s.bounding_box_type = "ground".into();
            s.bounding_box_ground_z = 0.0;
            t.spd = [0.0, 0.0, -120.0];
            t.spd_extend = true;
            t.bounce = true;
            t.bounce_factor = 0.5;
        });
        let state = SpawnerState { world_pos: [0.0, 0.0, 10.0], ..seeded() };
        let mut sim = Spawner::default();
        sim.advance(0, &config, &state, &frames);
        sim.fire();
        // Falling 2 a step from 10: below the ground at the sixth step.
        sim.advance(6, &config, &state, &frames);
        let p = &sim.particles[0];
        assert_eq!(p.pos[2], 0.0);
        // It bounced: now rising at half the speed.
        sim.advance(7, &config, &state, &frames);
        assert!((sim.particles[0].pos[2] - 1.0).abs() < 1e-9);

        // With "destroy" set it is gone on touching the ground.
        let mut destroying = config.clone();
        destroying.settings.destroy_at_bounding_box = true;
        let mut sim = Spawner::default();
        sim.advance(0, &destroying, &state, &frames);
        sim.fire();
        sim.advance(5, &destroying, &state, &frames);
        assert_eq!(sim.particles.len(), 1);
        sim.advance(6, &destroying, &state, &frames);
        assert!(sim.particles.is_empty());
    }

    #[test]
    fn spawn_regions_boxes_and_attractors() {
        let config = spawner(|s, _| {
            s.spawn_constant = false;
            s.spawn_amount = 200.0;
            s.spawn_region_use = true;
            s.spawn_region_type = "box".into();
            s.spawn_region_box_size = [10.0, 20.0, 2.0];
        });
        let state = SpawnerState { world_pos: [100.0, 0.0, 0.0], ..seeded() };
        let mut sim = Spawner::default();
        sim.advance(0, &config, &state, &frames);
        sim.fire();
        sim.advance(1, &config, &state, &frames);
        assert_eq!(sim.particles.len(), 200);
        assert!(sim.particles.iter().all(|p| (p.pos[0] - 100.0).abs() <= 5.0 && p.pos[1].abs() <= 10.0 && p.pos[2].abs() <= 1.0));
        let spread = sim.particles.iter().map(|p| p.pos[1]).fold(0.0f64, |m, y| m.max(y.abs()));
        assert!(spread > 7.0);

        // An attractor pulls particles towards it, by at most the force.
        let pulled = SpawnerState { attractor: Some([100.0, 0.0, 500.0]), force: 6.0, ..state.clone() };
        let before = sim.particles[0].pos[2];
        sim.advance(61, &config, &pulled, &frames);
        // Speed grows by 6/60 a step: after 60 steps it has risen 0.1 * (0 + 1 + ... + 59).
        let expected: f64 = (0..60).map(|i| 0.1 * i as f64).sum();
        assert!((sim.particles[0].pos[2] - before - expected).abs() < 1e-6);
    }

    #[test]
    fn sprites_animate_and_launch_directions_turn_with_the_spawner() {
        // Template animation: 8 images in reverse, 7 down to 0, at 7 a second.
        let config = spawner(|s, t| {
            s.spawn_constant = false;
            s.spawn_amount = 1.0;
            s.destroy_at_animation_finish = true;
            t.sprite_template_still_frame = false;
            t.sprite_animation_speed = 7.0;
        });
        let mut sim = Spawner::default();
        sim.advance(0, &config, &seeded(), &frames);
        sim.fire();
        sim.advance(1, &config, &seeded(), &frames);
        assert_eq!(sim.particles[0].frame, 7);
        sim.advance(31, &config, &seeded(), &frames);
        assert!((3..=4).contains(&sim.particles[0].frame), "{}", sim.particles[0].frame);
        // At the end of the animation the particle is removed.
        sim.advance(62, &config, &seeded(), &frames);
        assert!(sim.particles.is_empty());

        // Launching straight up at 60 a second; a spawner lying on its side
        // launches sideways.
        let launching = spawner(|s, t| {
            s.spawn_constant = false;
            s.spawn_amount = 1.0;
            t.angle_speed = 60.0;
        });
        let mut up = Spawner::default();
        up.advance(0, &launching, &seeded(), &frames);
        up.fire();
        up.advance(10, &launching, &seeded(), &frames);
        let p = up.particles[0].pos;
        assert!(p[0].abs() < 1e-9 && p[1].abs() < 1e-9 && (p[2] - 10.0).abs() < 1e-9, "{p:?}");
        let tipped = SpawnerState { rotation: [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]], ..seeded() };
        let mut side = Spawner::default();
        side.advance(0, &launching, &tipped, &frames);
        side.fire();
        side.advance(10, &launching, &tipped, &frames);
        let p = side.particles[0].pos;
        assert!((p[1] + 10.0).abs() < 1e-9 && p[2].abs() < 1e-9, "{p:?}");
        assert_eq!(launch_direction([0.0; 3]), [0.0, 0.0, 1.0]);
        // Tilting by 90 degrees lays the direction flat; turning around Z
        // alone leaves it pointing up.
        let flat = launch_direction([90.0, 0.0, 0.0]);
        assert!(flat[2].abs() < 1e-9 && (flat[0].abs() + flat[1].abs() - 1.0).abs() < 1e-9, "{flat:?}");
        assert!((launch_direction([0.0, 0.0, 90.0])[2] - 1.0).abs() < 1e-9);
    }
}
