//! Rotation points timelines get from their templates
//! (`temp_update_rot_point`, `tl_update_rot_point`).

use mi_core::{IdGenerator, ObjRef, SaveId, TempType, TlType};
use mi_format::project::{ProjectFile, Template, Timeline};
use mi_project::Project;

fn project() -> Project {
    let mut file = ProjectFile::new(0.0, 1.0);
    let mut scenery = Template::new(SaveId::new("SCENERY"), TempType::Scenery);
    scenery.scenery = ObjRef::id("RES");
    scenery.block_repeat_enable = true;
    scenery.block_repeat = [2.0, 1.0, 1.0];
    file.objects.templates.push(scenery);
    file.objects.templates.push(Template::new(SaveId::new("BLOCK"), TempType::Block));
    file.objects.templates.push(Template::new(SaveId::new("CUBE"), TempType::Cube));

    let mut add = |id: &str, kind: TlType, temp: &str, edit: &dyn Fn(&mut Timeline)| {
        let mut tl = Timeline::new(SaveId::new(id), kind, &file.defaults);
        tl.temp = ObjRef::id(temp);
        tl.rot_point = [1.0, 2.0, 3.0];
        edit(&mut tl);
        file.objects.timelines.push(tl);
    };
    add("SCN", TlType::Scenery, "SCENERY", &|_| {});
    add("BLK", TlType::Block, "BLOCK", &|_| {});
    add("CUB", TlType::Cube, "CUBE", &|_| {});
    add("CUSTOM", TlType::Cube, "CUBE", &|tl| tl.rot_point_custom = true);
    add("PART", TlType::Block, "BLOCK", &|tl| tl.part_of = ObjRef::id("SCN"));
    Project::from_file(file, IdGenerator::new(1)).0
}

#[test]
fn timelines_turn_around_their_templates_point() {
    let project = project();
    let size = |id: &SaveId| (id.as_str() == "RES").then_some([33.0, 38.0, 21.0]);
    let not_loaded = |_: &SaveId| None;
    let point = |id: &str, scenery: &mi_project::ScenerySize| {
        let tl = project.timelines().iter().find(|t| t.id.as_str() == id).unwrap();
        project.rot_point(tl, scenery)
    };
    // The middle of the floor of the (repeated) scenery.
    assert_eq!(point("SCN", &size), [2.0 * 33.0 * 8.0, 38.0 * 8.0, 0.0]);
    assert_eq!(point("SCN", &not_loaded), [0.0; 3]);
    assert_eq!(point("BLK", &size), [8.0, 8.0, 0.0]);
    assert_eq!(point("CUB", &size), [0.0, 0.0, -8.0]);
    // Custom points and parts of scenery keep their own.
    assert_eq!(point("CUSTOM", &size), [1.0, 2.0, 3.0]);
    assert_eq!(point("PART", &size), [1.0, 2.0, 3.0]);
}
