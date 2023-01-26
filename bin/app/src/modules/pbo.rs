use std::fs::{create_dir_all, File};

use hemtt_bin_error::Error;
use hemtt_pbo::{prefix::FILES, Prefix, WritablePbo};
use vfs::VfsFileType;

use crate::{addons::Location, context::Context};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Should the optional and compat PBOs be collapsed into the addons folder
pub enum Collapse {
    /// Yes, used for development
    Yes,
    /// No, used for build and release
    No,
}

pub fn build(ctx: &Context, collapse: Collapse) -> Result<(), Error> {
    ctx.addons()
        .to_vec()
        .iter()
        .map(|addon| {
            let mut pbo = WritablePbo::new();
            let target = ctx.out_folder();

            let pbo_name = addon.pbo_name(ctx.config().prefix());

            let target_pbo = {
                let mut path = match collapse {
                    Collapse::No => match addon.location() {
                        Location::Addons => target.join("addons").join(pbo_name),
                        Location::Optionals => {
                            if ctx.config().hemtt().build().optional_mod_folders() {
                                target
                                    .join("optionals")
                                    .join(format!("@{pbo_name}"))
                                    .join("addons")
                                    .join(pbo_name)
                            } else {
                                target.join(addon.location().to_string()).join(pbo_name)
                            }
                        }
                    },
                    Collapse::Yes => target.join("addons").join(pbo_name),
                };
                path.set_extension("pbo");
                path
            };
            create_dir_all(target_pbo.parent().unwrap())?;
            println!(
                "building `{}` => `{}`",
                addon.folder(),
                target_pbo.display()
            );

            pbo.add_property("hemtt", crate::VERSION.to_string());

            'entries: for entry in ctx.vfs().join(addon.folder()).unwrap().walk_dir().unwrap() {
                let entry = entry.unwrap();
                if entry.metadata().unwrap().file_type == VfsFileType::File {
                    if entry.filename() == "config.cpp"
                        && entry.parent().join("config.bin").unwrap().exists().unwrap()
                    {
                        continue;
                    }

                    if entry.filename() == "addon.toml" {
                        continue;
                    }

                    for exclude in ctx.config().files().exclude() {
                        if glob::Pattern::new(exclude)?.matches(entry.as_str()) {
                            continue 'entries;
                        }
                    }
                    if let Some(config) = addon.config() {
                        for exclude in config.exclude() {
                            if glob::Pattern::new(exclude)?.matches(
                                entry
                                    .as_str()
                                    .trim_start_matches(&format!("/{}/", addon.folder())),
                            ) {
                                continue 'entries;
                            }
                        }
                    }

                    if FILES.contains(&entry.filename().to_lowercase().as_str()) {
                        let prefix = Prefix::new(
                            &entry.read_to_string().unwrap(),
                            ctx.config().hemtt().pbo_prefix_allow_leading_slash(),
                        )?;
                        pbo.add_property("prefix", prefix.into_inner());
                        pbo.add_property("version", ctx.config().version().get()?.to_string());
                        continue;
                    }

                    let file = entry
                        .as_str()
                        .trim_start_matches(&format!("/{}/", addon.folder()))
                        .replace('/', "\\");
                    pbo.add_file(file, entry.open_file().unwrap()).unwrap();
                }
            }
            for header in ctx.config().properties() {
                pbo.add_property(header.0, header.1.clone());
            }
            if let Some(config) = addon.config() {
                for header in config.properties() {
                    pbo.add_property(header.0, header.1.clone());
                }
            }
            pbo.write(&mut File::create(target_pbo)?, true)?;
            Ok(())
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(())
}
