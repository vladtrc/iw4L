#[macro_export]
macro_rules! forward_t5_sound {
    () => {
        fn capture_snd_curves(
            &mut self,
            s: &ext_t5_fastfile::ZoneStream<'_>,
            rows: ext_t5_fastfile::Ptr,
            count: usize,
        ) -> ext_t5_fastfile::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.native_mut::<$crate::sound::T5SoundCapture>()
            {
                let result =
                    ext_t5_fastfile::AssetLinkSink::capture_snd_curves(capture, s, rows, count);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_snd_groups(
            &mut self,
            s: &ext_t5_fastfile::ZoneStream<'_>,
            rows: ext_t5_fastfile::Ptr,
            count: usize,
        ) -> ext_t5_fastfile::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.native_mut::<$crate::sound::T5SoundCapture>()
            {
                let result =
                    ext_t5_fastfile::AssetLinkSink::capture_snd_groups(capture, s, rows, count);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_loaded_sound(
            &mut self,
            s: &ext_t5_fastfile::ZoneStream<'_>,
            header: ext_t5_fastfile::Ptr,
            pcm: ext_t5_fastfile::Ptr,
            data_len: usize,
        ) -> ext_t5_fastfile::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.native_mut::<$crate::sound::T5SoundCapture>()
            {
                let result = ext_t5_fastfile::AssetLinkSink::capture_loaded_sound(
                    capture, s, header, pcm, data_len,
                );
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_last_loaded_to_sound_file(
            &mut self,
            file: ext_t5_fastfile::Ptr,
        ) -> ext_t5_fastfile::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.native_mut::<$crate::sound::T5SoundCapture>()
            {
                let result =
                    ext_t5_fastfile::AssetLinkSink::bind_last_loaded_to_sound_file(capture, file);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_sound(
            &mut self,
            s: &ext_t5_fastfile::ZoneStream<'_>,
            list: ext_t5_fastfile::Ptr,
            count: usize,
            head: Option<ext_t5_fastfile::Ptr>,
        ) -> ext_t5_fastfile::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.native_mut::<$crate::sound::T5SoundCapture>()
            {
                let result =
                    ext_t5_fastfile::AssetLinkSink::capture_sound(capture, s, list, count, head);
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_streamed_sound_file(
            &mut self,
            file: ext_t5_fastfile::Ptr,
            dir: &str,
            name: &str,
        ) -> ext_t5_fastfile::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.native_mut::<$crate::sound::T5SoundCapture>()
            {
                let result = ext_t5_fastfile::AssetLinkSink::bind_streamed_sound_file(
                    capture, file, dir, name,
                );
                sound.guard(result);
            }
            Ok(())
        }
    };
}

pub use crate::forward_t5_sound;
