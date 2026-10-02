//! Writes a small animated project, for trying the application without a
//! project made in the original program.
//!
//! ```sh
//! cargo run -p mi-format --example sample_project -- sample.miproject
//! ```

use mi_core::{Color, IdGenerator, ObjRef, TempType, TlType, Value, ValueId};
use mi_format::project::{Keyframe, Marker, ProjectFile, Template, Timeline};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "sample.miproject".to_owned());
    let mut ids = IdGenerator::new(2024);

    let mut project = ProjectFile::new(1.0, 1234.0);
    project.info.name = "Sample scene".to_owned();
    project.info.author = "mi-format".to_owned();
    project.info.description = "Three cubes orbiting a folder, a camera and a light.".to_owned();
    project.info.timeline.repeat = true;

    let mut cube_template = Template::new(ids.next_id(), TempType::Cube);
    cube_template.name = "Cube".to_owned();
    cube_template.shape.tex = ObjRef::default_resource();
    let cube_id = cube_template.id.clone();
    project.objects.templates.push(cube_template);

    let keyframe = |tl: &Timeline, position: i64, changes: &[(ValueId, Value)]| {
        let mut values = tl.default_values.clone();
        for (id, value) in changes {
            values[*id] = value.clone();
        }
        Keyframe { position, values }
    };
    let number = Value::Number;

    // A folder that spins once over the whole animation.
    let mut spinner = Timeline::new(ids.next_id(), TlType::Folder, &project.defaults);
    spinner.name = "Spinner".to_owned();
    spinner.parent_tree_index = Some(0);
    spinner.tree_extend = true;
    spinner.default_values[ValueId::PosZ] = number(32.0);
    spinner.keyframes = vec![
        keyframe(&spinner, 0, &[(ValueId::RotZ, number(0.0))]),
        keyframe(&spinner, 96, &[(ValueId::RotZ, number(360.0))]),
    ];
    let spinner_id = spinner.id.clone();
    project.objects.timelines.push(spinner);

    // Cubes around it, each bouncing with a different transition.
    let transitions = ["easeinoutquad", "easeoutbounce", "easeinoutelastic"];
    for (i, transition) in transitions.iter().enumerate() {
        let angle = (i as f64 / transitions.len() as f64) * std::f64::consts::TAU;
        let mut cube = Timeline::new(ids.next_id(), TlType::Cube, &project.defaults);
        cube.name = format!("Cube {}", i + 1);
        cube.temp = ObjRef::Id(cube_id.clone());
        cube.parent = spinner_id.clone();
        cube.parent_tree_index = Some(i as i64);
        cube.color_tag = Some(i as f64);
        cube.default_values[ValueId::PosX] = number((angle.cos() * 48.0).round());
        cube.default_values[ValueId::PosY] = number((angle.sin() * 48.0).round());
        let ease = Value::Str((*transition).to_owned());
        cube.keyframes = vec![
            keyframe(&cube, 0, &[(ValueId::PosZ, number(0.0)), (ValueId::Transition, ease.clone())]),
            keyframe(&cube, 24 + 8 * i as i64, &[(ValueId::PosZ, number(40.0)), (ValueId::Transition, ease.clone())]),
            keyframe(&cube, 48 + 16 * i as i64, &[(ValueId::PosZ, number(0.0))]),
        ];
        if i == 2 {
            // The last cube fades out and disappears.
            cube.keyframes.push(keyframe(&cube, 80, &[(ValueId::Alpha, number(0.0))]));
            cube.keyframes.push(keyframe(&cube, 88, &[(ValueId::Alpha, number(0.0)), (ValueId::Visible, Value::Bool(false))]));
        }
        project.objects.timelines.push(cube);
    }

    let mut light = Timeline::new(ids.next_id(), TlType::PointLight, &project.defaults);
    light.name = "Lamp".to_owned();
    light.parent_tree_index = Some(1);
    light.default_values[ValueId::PosZ] = number(96.0);
    light.keyframes = vec![
        keyframe(&light, 0, &[(ValueId::LightColor, Value::Color(Color::rgb(255, 200, 120)))]),
        keyframe(&light, 96, &[(ValueId::LightColor, Value::Color(Color::rgb(120, 200, 255)))]),
    ];
    project.objects.timelines.push(light);

    let mut camera = Timeline::new(ids.next_id(), TlType::Camera, &project.defaults);
    camera.name = "Camera".to_owned();
    camera.parent_tree_index = Some(2);
    camera.default_values[ValueId::CamRotate] = Value::Bool(true);
    camera.default_values[ValueId::CamRotateDistance] = number(220.0);
    camera.default_values[ValueId::CamRotateAngleZ] = number(20.0);
    camera.default_values[ValueId::PosZ] = number(32.0);
    camera.keyframes = vec![
        keyframe(&camera, 0, &[(ValueId::CamRotateAngleXy, number(-30.0)), (ValueId::CamFov, number(45.0))]),
        keyframe(&camera, 96, &[(ValueId::CamRotateAngleXy, number(30.0)), (ValueId::CamFov, number(60.0))]),
    ];
    project.objects.timelines.push(camera);

    project.markers = vec![
        Marker { id: ids.next_id(), position: 24.0, name: "First bounce".to_owned(), color: 2.0 },
        Marker { id: ids.next_id(), position: 80.0, name: "Fade".to_owned(), color: 5.0 },
    ];

    std::fs::write(&path, project.save())?;
    println!("Wrote {path}");
    Ok(())
}
