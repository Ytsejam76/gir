use super::{function_parameters::TransformationType, imports::Imports, *};
use crate::{codegen::Visibility, config::gobjects::GObject, env::Env, nameutil::*, traits::*};

use log::info;

#[derive(Debug, Default)]
pub struct Info {
    pub full_name: String,
    pub type_id: library::TypeId,
    pub name: String,
    pub functions: Vec<functions::Info>,
    pub specials: special_functions::Infos,
    pub visibility: Visibility,
}

impl Info {
    pub fn type_<'a>(&self, library: &'a library::Library) -> &'a library::Enumeration {
        let type_ = library
            .type_(self.type_id)
            .maybe_ref()
            .unwrap_or_else(|| panic!("{} is not an enumeration.", self.full_name));
        type_
    }
}

pub fn new(env: &Env, obj: &GObject, imports: &mut Imports) -> Option<Info> {
    info!("Analyzing enumeration {}", obj.name);

    if obj.status.ignored() {
        return None;
    }

    let enumeration_tid = env.library.find_type(0, &obj.name)?;
    let type_ = env.type_(enumeration_tid);
    let enumeration: &library::Enumeration = type_.maybe_ref()?;

    let name = split_namespace_name(&obj.name).1;

    if obj.status.need_generate() {
        // Mark the type as available within the enum namespace:
        imports.add_defined(&format!("crate::{}", name));

        let imports = &mut imports.with_defaults(enumeration.version, &None);
        imports.add("glib::translate::*");

        let has_get_quark = enumeration.error_domain.is_some();
        if has_get_quark {
            imports.add("glib::Quark");
            imports.add("glib::error::ErrorDomain");
        }

        let has_get_type = enumeration.glib_get_type.is_some();
        if has_get_type {
            imports.add("glib::Type");
            imports.add("glib::StaticType");
            imports.add("glib::value::FromValue");
            imports.add("glib::value::ToValue");
        }

        if obj.generate_display_trait {
            imports.add("std::fmt");
        }
    }

    let mut functions = functions::analyze(
        env,
        &enumeration.functions,
        enumeration_tid,
        false,
        false,
        obj,
        imports,
        None,
        None,
    );

    // Gir does not currently mark the first parameter of associated enum functions -
    // that are identical to its enum type - as instance parameter since most languages
    // do not support this.
    for f in &mut functions {
        if f.parameters.c_parameters.is_empty() {
            continue;
        }

        let first_param = &mut f.parameters.c_parameters[0];

        if first_param.typ == enumeration_tid {
            first_param.instance_parameter = true;

            let t = f
                .parameters
                .transformations
                .iter_mut()
                .find(|t| t.ind_c == 0)
                .unwrap();

            if let TransformationType::ToGlibScalar { name, .. } = &mut t.transformation_type {
                *name = "self".to_owned();
            } else {
                panic!(
                    "Enumeration function instance param must be passed as scalar, not {:?}",
                    t.transformation_type
                );
            }
        }
    }

    let specials = special_functions::extract(&mut functions, type_, obj);

    if obj.status.need_generate() {
        special_functions::analyze_imports(&specials, imports);
    }

    let info = Info {
        full_name: obj.name.clone(),
        type_id: enumeration_tid,
        name: name.to_owned(),
        functions,
        specials,
        visibility: obj.visibility,
    };

    Some(info)
}
