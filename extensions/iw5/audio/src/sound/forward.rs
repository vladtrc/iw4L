#[macro_export]
macro_rules! forward_iw5_sound {
    () => {
        fn capture_loaded_sound(
            &mut self,
            s: &ext_iw5_fastfile::ZoneStream<'_>,
            header: ext_iw5_fastfile::Ptr,
            pcm: ext_iw5_fastfile::Ptr,
            data_len: usize,
        ) -> ext_iw5_fastfile::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.native_mut::<$crate::sound::Iw5SoundCapture>()
            {
                let result = ext_iw5_fastfile::AssetLinkSink::capture_loaded_sound(
                    capture, s, header, pcm, data_len,
                );
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_last_loaded_to_sound_file(
            &mut self,
            file: ext_iw5_fastfile::Ptr,
        ) -> ext_iw5_fastfile::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.native_mut::<$crate::sound::Iw5SoundCapture>()
            {
                let result =
                    ext_iw5_fastfile::AssetLinkSink::bind_last_loaded_to_sound_file(capture, file);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_sound(
            &mut self,
            s: &ext_iw5_fastfile::ZoneStream<'_>,
            list: ext_iw5_fastfile::Ptr,
            count: usize,
            head: Option<ext_iw5_fastfile::Ptr>,
        ) -> ext_iw5_fastfile::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.native_mut::<$crate::sound::Iw5SoundCapture>()
            {
                let result =
                    ext_iw5_fastfile::AssetLinkSink::capture_sound(capture, s, list, count, head);
                sound.guard(result);
            }
            Ok(())
        }

        fn capture_snd_curve(
            &mut self,
            s: &ext_iw5_fastfile::ZoneStream<'_>,
            header: ext_iw5_fastfile::Ptr,
        ) -> ext_iw5_fastfile::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.native_mut::<$crate::sound::Iw5SoundCapture>()
            {
                let result = ext_iw5_fastfile::AssetLinkSink::capture_snd_curve(capture, s, header);
                sound.guard(result);
            }
            Ok(())
        }

        fn bind_streamed_sound_file(
            &mut self,
            file: ext_iw5_fastfile::Ptr,
            dir: &str,
            name: &str,
        ) -> ext_iw5_fastfile::Result<()> {
            if let Some(sound) = self.sound.as_mut()
                && let Some(capture) = sound.native_mut::<$crate::sound::Iw5SoundCapture>()
            {
                let result = ext_iw5_fastfile::AssetLinkSink::bind_streamed_sound_file(
                    capture, file, dir, name,
                );
                sound.guard(result);
            }
            Ok(())
        }
    };
}

pub use crate::forward_iw5_sound;
