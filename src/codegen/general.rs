use crate::{
    analysis::{
        self, general::StatusedTypeId, imports::Imports, namespaces, special_functions::TraitInfo,
    },
    config::{derives::Derive, Config},
    env::Env,
    gir_version::VERSION,
    nameutil::use_glib_type,
    version::Version,
    writer::primitives::tabs,
};
use std::{
    fmt::Display,
    io::{Result, Write},
};

pub fn start_comments(w: &mut dyn Write, conf: &Config) -> Result<()> {
    if conf.single_version_file.is_some() {
        return start_comments_no_version(w);
    }
    writeln!(
        w,
        "// This file was generated by gir (https://github.com/gtk-rs/gir @ {})
// from gir-files (https://github.com/gtk-rs/gir-files @ {})
// DO NOT EDIT",
        VERSION, conf.girs_version
    )
}

pub fn start_comments_no_version(w: &mut dyn Write) -> Result<()> {
    writeln!(
        w,
        "// This file was generated by gir (https://github.com/gtk-rs/gir)
// from gir-files (https://github.com/gtk-rs/gir-files)
// DO NOT EDIT"
    )
}

pub fn single_version_file(w: &mut dyn Write, conf: &Config) -> Result<()> {
    writeln!(
        w,
        "Generated by gir (https://github.com/gtk-rs/gir @ {})
from gir-files (https://github.com/gtk-rs/gir-files @ {})",
        VERSION, conf.girs_version
    )
}

pub fn uses(w: &mut dyn Write, env: &Env, imports: &Imports) -> Result<()> {
    writeln!(w)?;
    for (name, ref scope) in imports.iter() {
        if !scope.constraints.is_empty() {
            writeln!(
                w,
                "#[cfg(any({},feature = \"dox\"))]",
                scope.constraints.join(", ")
            )?;
            writeln!(
                w,
                "#[cfg_attr(feature = \"dox\", doc(cfg({})))]",
                scope.constraints.join(", ")
            )?;
        }

        version_condition(w, env, scope.version, false, 0)?;
        writeln!(w, "use {};", name)?;
    }

    Ok(())
}

fn format_parent_name(env: &Env, p: &StatusedTypeId) -> String {
    if p.type_id.ns_id == namespaces::MAIN {
        p.name.clone()
    } else {
        format!(
            "{krate}::{name}",
            krate = env.namespaces[p.type_id.ns_id].crate_name,
            name = p.name,
        )
    }
}

pub fn define_object_type(
    w: &mut dyn Write,
    env: &Env,
    type_name: &str,
    glib_name: &str,
    glib_class_name: Option<&str>,
    glib_func_name: &str,
    is_interface: bool,
    parents: &[StatusedTypeId],
) -> Result<()> {
    let sys_crate_name = env.main_sys_crate_name();
    let class_name = {
        if let Some(s) = glib_class_name {
            format!(", {}::{}", sys_crate_name, s)
        } else {
            "".to_string()
        }
    };

    let kind_name = if is_interface { "Interface" } else { "Object" };

    let parents: Vec<StatusedTypeId> = parents
        .iter()
        .filter(|p| !p.status.ignored())
        .cloned()
        .collect();

    writeln!(w)?;
    writeln!(w, "{} {{", use_glib_type(env, "wrapper!"))?;
    if parents.is_empty() {
        writeln!(
            w,
            "\tpub struct {}({}<{}::{}{}>);",
            type_name, kind_name, sys_crate_name, glib_name, class_name
        )?;
    } else if is_interface {
        let prerequisites: Vec<String> =
            parents.iter().map(|p| format_parent_name(env, p)).collect();

        writeln!(
            w,
            "\tpub struct {}(Interface<{}::{}{}>) @requires {};",
            type_name,
            sys_crate_name,
            glib_name,
            class_name,
            prerequisites.join(", ")
        )?;
    } else {
        let interfaces: Vec<String> = parents
            .iter()
            .filter(|p| {
                use crate::library::*;

                matches!(
                    *env.library.type_(p.type_id),
                    Type::Interface { .. } if !p.status.ignored()
                )
            })
            .map(|p| format_parent_name(env, p))
            .collect();

        let parents: Vec<String> = parents
            .iter()
            .filter(|p| {
                use crate::library::*;

                matches!(
                    *env.library.type_(p.type_id),
                    Type::Class { .. } if !p.status.ignored()
                )
            })
            .map(|p| format_parent_name(env, p))
            .collect();

        let mut parents_string = String::new();
        if !parents.is_empty() {
            parents_string.push_str(format!(" @extends {}", parents.join(", ")).as_str());
        }

        if !interfaces.is_empty() {
            if !parents.is_empty() {
                parents_string.push(',');
            }
            parents_string.push_str(format!(" @implements {}", interfaces.join(", ")).as_str());
        }

        writeln!(
            w,
            "\tpub struct {}(Object<{}::{}{}>){};",
            type_name, sys_crate_name, glib_name, class_name, parents_string,
        )?;
    }
    writeln!(w)?;
    writeln!(w, "\tmatch fn {{")?;
    writeln!(
        w,
        "\t\tget_type => || {}::{}(),",
        sys_crate_name, glib_func_name
    )?;
    writeln!(w, "\t}}")?;
    writeln!(w, "}}")?;

    Ok(())
}

fn define_boxed_type_internal(
    w: &mut dyn Write,
    env: &Env,
    type_name: &str,
    glib_name: &str,
    copy_fn: &TraitInfo,
    free_fn: &str,
    init_function_expression: &Option<String>,
    clear_function_expression: &Option<String>,
    get_type_fn: Option<&str>,
    derive: &[Derive],
) -> Result<()> {
    let sys_crate_name = env.main_sys_crate_name();
    writeln!(w, "{} {{", use_glib_type(env, "wrapper!"))?;

    derives(w, derive, 1)?;
    writeln!(
        w,
        "\tpub struct {}(Boxed<{}::{}>);",
        type_name, sys_crate_name, glib_name
    )?;
    writeln!(w)?;
    writeln!(w, "\tmatch fn {{")?;
    let mut_ov = if copy_fn.first_parameter_mut {
        "mut_override(ptr)"
    } else {
        "ptr"
    };
    writeln!(
        w,
        "\t\tcopy => |ptr| {}::{}({}),",
        sys_crate_name, copy_fn.glib_name, mut_ov
    )?;
    writeln!(w, "\t\tfree => |ptr| {}::{}(ptr),", sys_crate_name, free_fn)?;

    if let (Some(init_function_expression), Some(clear_function_expression)) =
        (init_function_expression, clear_function_expression)
    {
        writeln!(w, "\t\tinit => {},", init_function_expression,)?;
        writeln!(w, "\t\tclear => {},", clear_function_expression,)?;
    }

    if let Some(ref get_type_fn) = get_type_fn {
        writeln!(
            w,
            "\t\tget_type => || {}::{}(),",
            sys_crate_name, get_type_fn
        )?;
    }
    writeln!(w, "\t}}")?;
    writeln!(w, "}}")?;

    Ok(())
}

pub fn define_boxed_type(
    w: &mut dyn Write,
    env: &Env,
    type_name: &str,
    glib_name: &str,
    copy_fn: &TraitInfo,
    free_fn: &str,
    init_function_expression: &Option<String>,
    clear_function_expression: &Option<String>,
    get_type_fn: Option<(String, Option<Version>)>,
    derive: &[Derive],
) -> Result<()> {
    writeln!(w)?;

    if let Some((ref get_type_fn, get_type_version)) = get_type_fn {
        if get_type_version.is_some() {
            version_condition(w, env, get_type_version, false, 0)?;
            define_boxed_type_internal(
                w,
                env,
                type_name,
                glib_name,
                copy_fn,
                free_fn,
                init_function_expression,
                clear_function_expression,
                Some(&get_type_fn),
                derive,
            )?;

            writeln!(w)?;
            not_version_condition_no_dox(w, get_type_version, false, 0)?;
            define_boxed_type_internal(
                w,
                env,
                type_name,
                glib_name,
                copy_fn,
                free_fn,
                init_function_expression,
                clear_function_expression,
                None,
                derive,
            )?;
        } else {
            define_boxed_type_internal(
                w,
                env,
                type_name,
                glib_name,
                copy_fn,
                free_fn,
                init_function_expression,
                clear_function_expression,
                Some(&get_type_fn),
                derive,
            )?;
        }
    } else {
        define_boxed_type_internal(
            w,
            env,
            type_name,
            glib_name,
            copy_fn,
            free_fn,
            init_function_expression,
            clear_function_expression,
            None,
            derive,
        )?;
    }

    Ok(())
}

pub fn define_auto_boxed_type(
    w: &mut dyn Write,
    env: &Env,
    type_name: &str,
    glib_name: &str,
    init_function_expression: &Option<String>,
    clear_function_expression: &Option<String>,
    get_type_fn: &str,
    derive: &[Derive],
) -> Result<()> {
    let sys_crate_name = env.main_sys_crate_name();
    writeln!(w)?;
    writeln!(w, "{} {{", use_glib_type(env, "wrapper!"))?;
    derives(w, derive, 1)?;
    writeln!(
        w,
        "\tpub struct {}(Boxed<{}::{}>);",
        type_name, sys_crate_name, glib_name
    )?;
    writeln!(w)?;
    writeln!(w, "\tmatch fn {{")?;
    writeln!(
        w,
        "\t\tcopy => |ptr| {}({}::{}(), ptr as *mut _) as *mut {}::{},",
        use_glib_type(env, "gobject_ffi::g_boxed_copy"),
        sys_crate_name,
        get_type_fn,
        sys_crate_name,
        glib_name
    )?;
    writeln!(
        w,
        "\t\tfree => |ptr| {}({}::{}(), ptr as *mut _),",
        use_glib_type(env, "gobject_ffi::g_boxed_free"),
        sys_crate_name,
        get_type_fn
    )?;

    if let (Some(init_function_expression), Some(clear_function_expression)) =
        (init_function_expression, clear_function_expression)
    {
        writeln!(w, "\t\tinit => {},", init_function_expression,)?;
        writeln!(w, "\t\tclear => {},", clear_function_expression,)?;
    }

    writeln!(
        w,
        "\t\tget_type => || {}::{}(),",
        sys_crate_name, get_type_fn
    )?;
    writeln!(w, "\t}}")?;
    writeln!(w, "}}")?;

    Ok(())
}

fn define_shared_type_internal(
    w: &mut dyn Write,
    env: &Env,
    type_name: &str,
    glib_name: &str,
    ref_fn: &str,
    unref_fn: &str,
    get_type_fn: Option<&str>,
    derive: &[Derive],
) -> Result<()> {
    let sys_crate_name = env.main_sys_crate_name();
    writeln!(w, "{} {{", use_glib_type(env, "wrapper!"))?;
    derives(w, derive, 1)?;
    writeln!(
        w,
        "\tpub struct {}(Shared<{}::{}>);",
        type_name, sys_crate_name, glib_name
    )?;
    writeln!(w)?;
    writeln!(w, "\tmatch fn {{")?;
    writeln!(w, "\t\tref => |ptr| {}::{}(ptr),", sys_crate_name, ref_fn)?;
    writeln!(
        w,
        "\t\tunref => |ptr| {}::{}(ptr),",
        sys_crate_name, unref_fn
    )?;
    if let Some(ref get_type_fn) = get_type_fn {
        writeln!(
            w,
            "\t\tget_type => || {}::{}(),",
            sys_crate_name, get_type_fn
        )?;
    }
    writeln!(w, "\t}}")?;
    writeln!(w, "}}")?;

    Ok(())
}

pub fn define_shared_type(
    w: &mut dyn Write,
    env: &Env,
    type_name: &str,
    glib_name: &str,
    ref_fn: &str,
    unref_fn: &str,
    get_type_fn: Option<(String, Option<Version>)>,
    derive: &[Derive],
) -> Result<()> {
    writeln!(w)?;

    if let Some((ref get_type_fn, get_type_version)) = get_type_fn {
        if get_type_version.is_some() {
            version_condition(w, env, get_type_version, false, 0)?;
            define_shared_type_internal(
                w,
                env,
                type_name,
                glib_name,
                ref_fn,
                unref_fn,
                Some(&get_type_fn),
                derive,
            )?;

            writeln!(w)?;
            not_version_condition_no_dox(w, get_type_version, false, 0)?;
            define_shared_type_internal(
                w, env, type_name, glib_name, ref_fn, unref_fn, None, derive,
            )?;
        } else {
            define_shared_type_internal(
                w,
                env,
                type_name,
                glib_name,
                ref_fn,
                unref_fn,
                Some(&get_type_fn),
                derive,
            )?;
        }
    } else {
        define_shared_type_internal(w, env, type_name, glib_name, ref_fn, unref_fn, None, derive)?;
    }

    Ok(())
}

pub fn cfg_deprecated(
    w: &mut dyn Write,
    env: &Env,
    deprecated: Option<Version>,
    commented: bool,
    indent: usize,
) -> Result<()> {
    if let Some(s) = cfg_deprecated_string(deprecated, env, commented, indent) {
        writeln!(w, "{}", s)?;
    }
    Ok(())
}

pub fn cfg_deprecated_string(
    deprecated: Option<Version>,
    env: &Env,
    commented: bool,
    indent: usize,
) -> Option<String> {
    let comment = if commented { "//" } else { "" };
    if env.is_too_low_version(deprecated) {
        Some(format!("{}{}#[deprecated]", tabs(indent), comment))
    } else if let Some(v) = deprecated {
        Some(format!(
            "{}{}#[cfg_attr({}, deprecated)]",
            tabs(indent),
            comment,
            v.to_cfg()
        ))
    } else {
        None
    }
}

pub fn version_condition(
    w: &mut dyn Write,
    env: &Env,
    version: Option<Version>,
    commented: bool,
    indent: usize,
) -> Result<()> {
    if let Some(s) = version_condition_string(env, version, commented, indent) {
        writeln!(w, "{}", s)?;
    }
    Ok(())
}

pub fn version_condition_no_doc(
    w: &mut dyn Write,
    env: &Env,
    version: Option<Version>,
    commented: bool,
    indent: usize,
) -> Result<()> {
    match version {
        Some(v) if v > env.config.min_cfg_version => {
            if let Some(s) = cfg_condition_string_no_doc(&Some(v.to_cfg()), commented, indent) {
                writeln!(w, "{}", s)?
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn version_condition_string(
    env: &Env,
    version: Option<Version>,
    commented: bool,
    indent: usize,
) -> Option<String> {
    match version {
        Some(v) if v > env.config.min_cfg_version => {
            cfg_condition_string(&Some(v.to_cfg()), commented, indent)
        }
        _ => None,
    }
}

pub fn not_version_condition(
    w: &mut dyn Write,
    version: Option<Version>,
    commented: bool,
    indent: usize,
) -> Result<()> {
    if let Some(s) = version.and_then(|v| {
        cfg_condition_string(&Some(format!("not({})", v.to_cfg())), commented, indent)
    }) {
        writeln!(w, "{}", s)?;
    }
    Ok(())
}

pub fn not_version_condition_no_dox(
    w: &mut dyn Write,
    version: Option<Version>,
    commented: bool,
    indent: usize,
) -> Result<()> {
    if let Some(v) = version {
        let comment = if commented { "//" } else { "" };
        let s = format!(
            "{}{}#[cfg(not(any({}, feature = \"dox\")))]",
            tabs(indent),
            comment,
            v.to_cfg()
        );
        writeln!(w, "{}", s)?;
    }
    Ok(())
}

pub fn cfg_condition(
    w: &mut dyn Write,
    cfg_condition: &Option<String>,
    commented: bool,
    indent: usize,
) -> Result<()> {
    let s = cfg_condition_string(cfg_condition, commented, indent);
    if let Some(s) = s {
        writeln!(w, "{}", s)?;
    }
    Ok(())
}

pub fn cfg_condition_string_no_doc(
    cfg_condition: &Option<String>,
    commented: bool,
    indent: usize,
) -> Option<String> {
    match cfg_condition.as_ref() {
        Some(v) => {
            let comment = if commented { "//" } else { "" };
            Some(format!(
                "{0}{1}#[cfg(any({2}, feature = \"dox\"))]",
                tabs(indent),
                comment,
                v
            ))
        }
        None => None,
    }
}

pub fn cfg_condition_string(
    cfg_condition: &Option<String>,
    commented: bool,
    indent: usize,
) -> Option<String> {
    match cfg_condition.as_ref() {
        Some(v) => {
            let comment = if commented { "//" } else { "" };
            Some(format!(
                "{0}{1}#[cfg(any({2}, feature = \"dox\"))]\n\
                 {0}{1}#[cfg_attr(feature = \"dox\", doc(cfg({2})))]",
                tabs(indent),
                comment,
                v
            ))
        }
        None => None,
    }
}

pub fn derives(w: &mut dyn Write, derives: &[Derive], indent: usize) -> Result<()> {
    for derive in derives {
        let s = match &derive.cfg_condition {
            Some(condition) => format!(
                "#[cfg_attr({}, derive({}))]",
                condition,
                derive.names.join(", ")
            ),
            None => format!("#[derive({})]", derive.names.join(", ")),
        };
        writeln!(w, "{}{}", tabs(indent), s)?;
    }
    Ok(())
}

pub fn doc_alias(w: &mut dyn Write, name: &str, comment_prefix: &str, indent: usize) -> Result<()> {
    writeln!(
        w,
        "{}{}#[doc(alias = \"{}\")]",
        tabs(indent),
        comment_prefix,
        name,
    )
}

pub fn doc_hidden(
    w: &mut dyn Write,
    doc_hidden: bool,
    comment_prefix: &str,
    indent: usize,
) -> Result<()> {
    if doc_hidden {
        writeln!(w, "{}{}#[doc(hidden)]", tabs(indent), comment_prefix)
    } else {
        Ok(())
    }
}

pub fn write_vec<T: Display>(w: &mut dyn Write, v: &[T]) -> Result<()> {
    for s in v {
        writeln!(w, "{}", s)?;
    }
    Ok(())
}

pub fn declare_default_from_new(
    w: &mut dyn Write,
    env: &Env,
    name: &str,
    functions: &[analysis::functions::Info],
) -> Result<()> {
    if let Some(func) = functions.iter().find(|f| {
        !f.visibility.hidden()
            && f.status.need_generate()
            && f.name == "new"
            && f.parameters.rust_parameters.is_empty()
    }) {
        writeln!(w)?;
        version_condition(w, env, func.version, false, 0)?;
        writeln!(w, "impl Default for {} {{", name)?;
        writeln!(w, "    fn default() -> Self {{")?;
        writeln!(w, "        Self::new()")?;
        writeln!(w, "    }}")?;
        writeln!(w, "}}")?;
    }

    Ok(())
}

/// Escapes string in format suitable for placing inside double quotes.
pub fn escape_string(s: &str) -> String {
    let mut es = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            '\"' | '\\' => es.push('\\'),
            _ => (),
        }
        es.push(c)
    }
    es
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string(""), "");
        assert_eq!(escape_string("no escaping here"), "no escaping here");
        assert_eq!(escape_string(r#"'"\"#), r#"'\"\\"#);
    }
}
