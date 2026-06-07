use anyhow::{Context, Result};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player as RodioPlayer};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub struct Player {
    _sink: MixerDeviceSink,
    player: RodioPlayer,
}

impl Player {
    pub fn new() -> Result<Self> {
        let mut sink = DeviceSinkBuilder::open_default_sink()
            .context("open audio output device")?;
        sink.log_on_drop(false);
        let player = RodioPlayer::connect_new(sink.mixer());
        Ok(Player { _sink: sink, player })
    }

    pub fn play_file(&self, path: &Path) -> Result<()> {
        let file = File::open(path)
            .with_context(|| format!("open audio file {}", path.display()))?;
        let source = Decoder::new(BufReader::new(file))
            .with_context(|| format!("decode audio file {}", path.display()))?;
        self.player.append(source);
        Ok(())
    }

    pub fn sleep_until_end(&self) {
        self.player.sleep_until_end();
    }
}
