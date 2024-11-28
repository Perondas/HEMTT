use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU16, Ordering},
        RwLock,
    },
};

use hemtt_config::{analyze::lint_check, parse, rapify::Rapify, Config};
use hemtt_preprocessor::Processor;
use hemtt_workspace::{
    addons::{Addon, Location},
    WorkspacePath,
};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use vfs::VfsFileType;

use crate::{context::Context, error::Error, progress::progress_bar, report::Report};

use super::Module;

#[derive(Default)]
pub struct AddonConfigs(RwLock<HashMap<(String, Location), Config>>);

impl std::ops::Deref for AddonConfigs {
    type Target = RwLock<HashMap<(String, Location), Config>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Default)]
pub struct Rapifier;

impl Module for Rapifier {
    fn name(&self) -> &'static str {
        "Rapifier"
    }

    fn check(&self, ctx: &Context) -> Result<Report, Error> {
        let mut report = Report::new();
        report.extend(lint_check(ctx.config().lints().config().clone()));
        Ok(report)
    }

    fn pre_build(&self, ctx: &Context) -> Result<Report, Error> {
        ctx.state().set(AddonConfigs::default());
        let mut report = Report::new();
        let counter = AtomicU16::new(0);
        let glob_options = glob::MatchOptions {
            require_literal_separator: true,
            ..Default::default()
        };
        let mut entries = Vec::new();
        ctx.addons()
            .iter()
            .map(|addon| {
                let mut globs = Vec::new();
                if let Some(config) = addon.config() {
                    if !config.rapify().enabled() {
                        debug!("rapify disabled for {}", addon.name());
                        return Ok(());
                    }
                    for file in config.rapify().exclude() {
                        globs.push(glob::Pattern::new(file)?);
                    }
                }
                for entry in ctx.workspace_path().join(addon.folder())?.walk_dir()? {
                    if entry.metadata()?.file_type == VfsFileType::File
                        && can_rapify(entry.as_str())
                    {
                        if globs
                            .iter()
                            .any(|pat| pat.matches_with(entry.as_str(), glob_options))
                        {
                            debug!("skipping {}", entry.as_str());
                            continue;
                        }
                        entries.push((addon, entry));
                    }
                }
                Ok(())
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let progress = progress_bar(entries.len() as u64).with_message("Rapifying Configs");
        let reports = entries
            .par_iter()
            .map(|(addon, entry)| {
                let report = rapify(addon, entry, ctx)?;
                counter.fetch_add(1, Ordering::Relaxed);
                progress.inc(1);
                Ok(report)
            })
            .collect::<Result<Vec<Report>, Error>>()?;

        for new_report in reports {
            report.merge(new_report);
        }

        progress.finish_and_clear();
        info!("Rapified {} addon configs", counter.load(Ordering::Relaxed));
        Ok(report)
    }
}

#[allow(clippy::too_many_lines)]
pub fn rapify(addon: &Addon, path: &WorkspacePath, ctx: &Context) -> Result<Report, Error> {
    let mut report = Report::new();
    let processed = match Processor::run(path) {
        Ok(processed) => processed,
        Err((_, hemtt_preprocessor::Error::Code(e))) => {
            report.push(e);
            return Ok(report);
        }
        Err((_, e)) => {
            return Err(e.into());
        }
    };
    for warning in processed.warnings() {
        report.push(warning.clone());
    }
    let configreport = match parse(Some(ctx.config()), &processed) {
        Ok(configreport) => configreport,
        Err(errors) => {
            for e in &errors {
                report.push(e.clone());
            }
            return Ok(report);
        }
    };
    configreport.warnings().into_iter().for_each(|e| {
        report.push(e.clone());
    });
    configreport.errors().into_iter().for_each(|e| {
        report.push(e.clone());
    });
    if !configreport.errors().is_empty() {
        return Ok(report);
    }
    let out = if std::path::Path::new(&path.filename())
        .extension()
        .map_or(false, |ext| ext.eq_ignore_ascii_case("cpp"))
    {
        if path.filename() == "config.cpp" {
            let (version, cfgpatch) = configreport.required_version();
            let mut file = path;
            let mut span = 0..0;
            if let Some(cfgpatch) = cfgpatch {
                let map = processed
                    .mapping(cfgpatch.name().span.start)
                    .expect("mapping should exist");
                file = map.original().path();
                span = map.original().start().0..map.original().end().0;
            }
            addon
                .build_data()
                .set_required_version(version, file.to_owned(), span);
            ctx.state()
                .get::<AddonConfigs>()
                .write()
                .expect("state is poisoned")
                .insert(
                    (addon.name().to_owned(), *addon.location()),
                    configreport.config().clone(),
                );
        }
        path.with_extension("bin")?
    } else {
        path.to_owned()
    };
    if processed.no_rapify() {
        debug!(
            "skipping rapify for {}, as instructed by preprocessor",
            out.as_str()
        );
        return Ok(report);
    }
    let mut output = match out.create_file() {
        Ok(output) => output,
        Err(e) => {
            return Err(e.into());
        }
    };
    if let Err(e) = configreport.config().rapify(&mut output, 0) {
        return Err(e.into());
    }
    Ok(report)
}

pub fn can_rapify(path: &str) -> bool {
    let pathbuf = PathBuf::from(path);
    let ext = pathbuf
        .extension()
        .unwrap_or_else(|| std::ffi::OsStr::new(""))
        .to_str()
        .expect("osstr should be valid utf8");
    if ext == "cpp" && pathbuf.file_name() != Some(std::ffi::OsStr::new("config.cpp")) {
        warn!(
            "{} - cpp files other than config.cpp are usually not intentional. use hpp for includes",
            path.trim_start_matches('/')
        );
    }
    ["cpp", "rvmat", "ext"].contains(&ext)
}
