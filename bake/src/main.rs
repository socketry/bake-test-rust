// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake::{Registry, Result};

fn main() -> Result<()> {
    Registry::discover()?.run()
}

#[cfg(test)]
mod tests {
    use bake::{Arguments, Context, Parameter, Registry, Result, Task, Value};
    use socketry_project::after_version_bump;
    use std::fs;

    fn license_update(context: &mut Context, _arguments: &Arguments) -> Result<Value> {
        fs::write(context.root().join("license-called"), "yes")?;
        Ok(Value::Null)
    }

    fn releases_update(context: &mut Context, arguments: &Arguments) -> Result<Value> {
        let version = arguments.required::<String>("version")?;
        fs::write(context.root().join("release-version"), version)?;
        Ok(Value::Null)
    }

    fn readme_update(context: &mut Context, _arguments: &Arguments) -> Result<Value> {
        fs::write(context.root().join("readme-called"), "yes")?;
        Ok(Value::Null)
    }

    #[test]
    fn updates_release_files_in_order_with_the_bumped_version() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "bake-test-rust-bake-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create temporary Bake project");
        let mut registry = Registry::new();
        registry
            .register(Task::new("license:update", "", Vec::new(), license_update))
            .expect("register license task");
        registry
            .register(Task::new(
                "releases:update",
                "",
                vec![Parameter::new::<String>("version")],
                releases_update,
            ))
            .expect("register releases task");
        registry
            .register(Task::new("readme:update", "", Vec::new(), readme_update))
            .expect("register readme task");
        let mut context = registry.context(&root);

        after_version_bump(&mut context, "1.2.3".to_owned()).expect("run release update tasks");

        assert_eq!(
            fs::read_to_string(root.join("license-called")).unwrap(),
            "yes"
        );
        assert_eq!(
            fs::read_to_string(root.join("release-version")).unwrap(),
            "v1.2.3"
        );
        assert_eq!(
            fs::read_to_string(root.join("readme-called")).unwrap(),
            "yes"
        );
        drop(context);
        fs::remove_dir_all(root).expect("remove temporary Bake project");
    }
}

#[path = "bake_generated_tasks/mod.rs"]
mod bake_generated_tasks;
