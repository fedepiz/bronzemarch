use macroquad::prelude as mq;
use std::collections::HashMap;

pub struct Assets {
    fonts: HashMap<String, mq::Font>,
    textures: HashMap<String, mq::Texture2D>,
}

impl Assets {
    pub async fn load() -> anyhow::Result<Assets> {
        Ok(Assets {
            fonts: Self::load_fonts().unwrap(),
            textures: Self::load_textures().await?,
        })
    }

    fn load_fonts() -> anyhow::Result<HashMap<String, mq::Font>> {
        let mut map = HashMap::default();
        for entry in std::fs::read_dir("assets/fonts")? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = Self::file_name(&entry);
            let data = std::fs::read(entry.path())?;
            let font = mq::load_ttf_font_from_bytes(&data)?;
            map.insert(name, font);
        }

        Ok(map)
    }

    async fn load_textures() -> anyhow::Result<HashMap<String, mq::Texture2D>> {
        let mut map = HashMap::default();
        for entry in std::fs::read_dir("assets/gfx")? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = Self::file_name(&entry);
            let texture = mq::load_texture(entry.path().to_str().unwrap()).await?;
            map.insert(name, texture);
        }

        Ok(map)
    }

    fn file_name(entry: &std::fs::DirEntry) -> String {
        entry
            .file_name()
            .to_string_lossy()
            .to_string()
            .split('.')
            .next()
            .unwrap()
            .to_string()
    }

    pub fn font(&self, name: &str) -> &mq::Font {
        self.fonts.get(name).unwrap()
    }

    pub fn texture(&self, name: &str) -> &mq::Texture2D {
        self.textures.get(name).unwrap()
    }
}
