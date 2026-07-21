use aam_rs::define_aam_loader;
use std::path::Path;

define_aam_loader! {
    name: ListenerLoader,
    dir: "~/.config/nothypridle/rules/",

    list: {
        keep_alive_processes: String,
    },
    opt: {
        on_timeout: String,
        on_resume: String,
        music_process_name: String,
    },
    req: {
        timeout: u32,
        max_cpu_usage: f32,
        max_gpu_usage: f32,
        min_ram_mb: u32,
        min_vram_mb: u32,
        music_playing: bool,
        fullscreen: bool,
    },
}

impl ListenerLoader {
    /// Loads every `.aam` rule file from `rules_dir`.
    ///
    /// If `rules_dir/schema.aam` exists, its content is prepended to each rule
    /// file before parsing.  This injects the `@schema Rule { … }` definition
    /// so that AAM validates every rule's fields (types, required/optional)
    /// against the shared schema.  Each rule file should therefore start with
    /// `@derive schema.aam::Rule` to declare which schema it follows.
    ///
    /// If `schema.aam` is absent, falls back to plain `load_dir` behaviour
    /// (no schema validation) for backward compatibility.
    pub fn load_dir_with_schema(&mut self, rules_dir: &Path) -> anyhow::Result<()> {
        let schema_path = rules_dir.join("schema.aam");
        if !schema_path.exists() {
            return self.load_dir(rules_dir);
        }

        let schema_content = std::fs::read_to_string(&schema_path).map_err(|e| {
            anyhow::anyhow!("Failed to read schema '{}': {e}", schema_path.display())
        })?;

        let files: Vec<_> = std::fs::read_dir(rules_dir)
            .map_err(|e| {
                anyhow::anyhow!("Failed to read rules dir '{}': {e}", rules_dir.display())
            })?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|p| {
                p.is_file()
                    && p.extension().and_then(|s| s.to_str()) == Some("aam")
                    && p.file_name().and_then(|s| s.to_str()) != Some("config.aam")
                    && p.file_name().and_then(|s| s.to_str()) != Some("schema.aam")
            })
            .collect();

        for file in files {
            let rule_content = std::fs::read_to_string(&file)
                .map_err(|e| anyhow::anyhow!("Failed to read rule '{}': {e}", file.display()))?;

            let combined = format!("{schema_content}\n{rule_content}");

            // Write the combined content to a temporary file in the same
            // directory so that `AAM::load` sets `source_dir` correctly for
            // `@derive schema.aam::Rule` path resolution.
            let file_name = file
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("rule.aam");
            let temp_path = rules_dir.join(format!(".{file_name}.nhidle-tmp"));
            std::fs::write(&temp_path, &combined).map_err(|e| {
                anyhow::anyhow!("Failed to write temp file '{}': {e}", temp_path.display())
            })?;

            let result = self.load_aam(&temp_path);
            // Clean up the temp file AND any AOT cache file that `AAM::load`
            // may have created next to it (`.aam.bin`).
            let _ = std::fs::remove_file(&temp_path);
            let _ = std::fs::remove_file(temp_path.with_extension("aam.bin"));
            result?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &std::path::Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn schema_content() -> &'static str {
        "@schema Rule {\n\
         id: string\n\
         timeout: i32\n\
         max_cpu_usage: f64\n\
         max_gpu_usage: f64\n\
         min_ram_mb: i32\n\
         min_vram_mb: i32\n\
         music_playing: bool\n\
         fullscreen: bool\n\
         on_timeout*: string\n\
         on_resume*: string\n\
         music_process_name*: string\n\
         keep_alive_processes*: list<string>\n\
         }\n"
    }

    #[test]
    fn load_dir_with_schema_accepts_valid_rule() {
        let dir = std::env::temp_dir().join("nhidle-config-test-valid");
        let _ = std::fs::remove_dir_all(&dir);
        write(&dir, "schema.aam", schema_content());
        write(
            &dir,
            "dim.aam",
            "id = dim\n\
             @derive schema.aam::Rule\n\
             timeout = 120\n\
             max_cpu_usage = 15.0\n\
             max_gpu_usage = 15.0\n\
             min_ram_mb = 1024\n\
             min_vram_mb = 256\n\
             music_playing = true\n\
             fullscreen = true\n\
             on_timeout = brightnessctl\n\
             keep_alive_processes = [ffmpeg]",
        );

        let mut loader = ListenerLoader::new();
        loader
            .load_dir_with_schema(&dir)
            .expect("valid rule should load");

        assert_eq!(loader.get_all_ids(), vec!["dim".to_string()]);
        assert_eq!(loader.timeout("dim").unwrap(), 120);
        assert_eq!(loader.max_cpu_usage("dim").unwrap(), 15.0);
        assert_eq!(
            loader.on_timeout("dim").unwrap(),
            Some("brightnessctl".to_string())
        );
        assert_eq!(
            loader.keep_alive_processes("dim").unwrap(),
            vec!["ffmpeg".to_string()]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_dir_with_schema_rejects_missing_required_field() {
        let dir = std::env::temp_dir().join("nhidle-config-test-missing");
        let _ = std::fs::remove_dir_all(&dir);
        write(&dir, "schema.aam", schema_content());
        // Missing timeout, max_cpu_usage, etc.
        write(
            &dir,
            "bad.aam",
            "id = bad\n@derive schema.aam::Rule\non_timeout = \"echo hi\"",
        );

        let mut loader = ListenerLoader::new();
        let result = loader.load_dir_with_schema(&dir);
        assert!(result.is_err(), "missing required fields should fail");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_dir_with_schema_rejects_wrong_type() {
        let dir = std::env::temp_dir().join("nhidle-config-test-wrongtype");
        let _ = std::fs::remove_dir_all(&dir);
        write(&dir, "schema.aam", schema_content());
        write(
            &dir,
            "wrong.aam",
            "id = wrong\n\
             @derive schema.aam::Rule\n\
             timeout = \"not a number\"\n\
             max_cpu_usage = 15.0\n\
             max_gpu_usage = 15.0\n\
             min_ram_mb = 1024\n\
             min_vram_mb = 256\n\
             music_playing = true\n\
             fullscreen = true",
        );

        let mut loader = ListenerLoader::new();
        let result = loader.load_dir_with_schema(&dir);
        assert!(result.is_err(), "wrong type should fail");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_dir_with_schema_skips_schema_file_itself() {
        let dir = std::env::temp_dir().join("nhidle-config-test-skip");
        let _ = std::fs::remove_dir_all(&dir);
        write(&dir, "schema.aam", schema_content());
        write(
            &dir,
            "ok.aam",
            "id = ok\n\
             @derive schema.aam::Rule\n\
             timeout = 60\n\
             max_cpu_usage = 10.0\n\
             max_gpu_usage = 10.0\n\
             min_ram_mb = 256\n\
             min_vram_mb = 64\n\
             music_playing = false\n\
             fullscreen = false",
        );

        let mut loader = ListenerLoader::new();
        loader.load_dir_with_schema(&dir).expect("should load");

        // schema.aam must NOT appear as a rule id
        let ids = loader.get_all_ids();
        assert_eq!(ids, vec!["ok".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_dir_with_schema_falls_back_without_schema_file() {
        let dir = std::env::temp_dir().join("nhidle-config-test-fallback");
        let _ = std::fs::remove_dir_all(&dir);
        // No schema.aam — should behave like plain load_dir
        write(
            &dir,
            "plain.aam",
            "id = plain\ntimeout = 30\nmax_cpu_usage = 5.0\nmax_gpu_usage = 5.0\nmin_ram_mb = 100\nmin_vram_mb = 50\nmusic_playing = false\nfullscreen = false",
        );

        let mut loader = ListenerLoader::new();
        loader
            .load_dir_with_schema(&dir)
            .expect("fallback should work");

        assert_eq!(loader.get_all_ids(), vec!["plain".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_dir_with_schema_cleans_up_temp_files() {
        let dir = std::env::temp_dir().join("nhidle-config-test-cleanup");
        let _ = std::fs::remove_dir_all(&dir);
        write(&dir, "schema.aam", schema_content());
        write(
            &dir,
            "rule.aam",
            "id = rule\n\
             @derive schema.aam::Rule\n\
             timeout = 90\n\
             max_cpu_usage = 20.0\n\
             max_gpu_usage = 20.0\n\
             min_ram_mb = 512\n\
             min_vram_mb = 128\n\
             music_playing = false\n\
             fullscreen = false",
        );

        let mut loader = ListenerLoader::new();
        loader.load_dir_with_schema(&dir).unwrap();

        // No temp files or AOT cache files should remain
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.contains("nhidle-tmp") || name.ends_with(".bin")
            })
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp/cache files should be cleaned up, found: {leftovers:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
