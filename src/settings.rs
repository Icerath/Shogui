use std::sync::Mutex;
use std::{
    io::{self, BufReader, BufWriter},
    path::PathBuf,
};

macro_rules! define_settings {
    ($($field:ident: $ty:ty = $default:expr),* $(,)?) => {
        #[derive(serde::Deserialize, serde::Serialize)]
        pub struct Settings {
            $(
                $field: Mutex<Option<$ty>>,
            )*
        }

        impl Settings {
        $(
            pub fn $field(&self) -> $ty {
                self.$field.lock().unwrap().clone().unwrap_or_else(|| $default)
            }
            pub fn ${concat(set_, $field)}(&self, value: $ty) -> io::Result<()> {
                {
                let current = &mut *self.$field.lock().unwrap();
                    match current {
                        Some(current) if *current == value => return Ok(()),
                        _ => {
                            *current = Some(value);
                        }
                    };
                }
                self.write()
            }
        )*
            fn default_() -> Self {
                Self {
                    $($field: Mutex::new(Some($default)),)*
                }
            }
        }

    };
}

define_settings! {
    port: Option<u16> = None,
    dark_mode: bool = true,
}

impl Settings {
    pub fn read() -> io::Result<Self> {
        let path = path()?;
        if !path.exists() {
            _ = std::fs::create_dir(path.parent().unwrap());
            std::fs::File::create_new(&path)?;
            let settings = Self::default_();
            settings.write()?;
            return Ok(settings);
        }
        Ok(serde_json::from_reader(BufReader::new(std::fs::File::open(path)?))?)
    }
    fn write(&self) -> io::Result<()> {
        Ok(serde_json::to_writer(
            BufWriter::new(std::fs::File::options().write(true).open(path()?)?),
            self,
        )?)
    }
}

fn path() -> io::Result<PathBuf> {
    Ok(dirs::data_dir()
        .ok_or_else(|| io::Error::other("failed to write to settings"))?
        .join("Shogui/settings.json"))
}
