#[derive(Clone, Debug, PartialEq)]
pub struct T5DestructibleDef {
    pub name: String,
    pub model: String,
    pub pieces: Vec<T5DestructiblePiece>,
    pub client_only: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct T5DestructiblePiece {
    pub stages: [T5DestructibleStage; 5],
    pub parent_piece: u8,
    pub parent_damage_percent: f32,
    pub bullet_damage_scale: f32,
    pub explosive_damage_scale: f32,
    pub health: i32,
    pub hide_bones: [u32; 5],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct T5DestructibleStage {
    pub show_bone: Option<String>,
    pub break_health: f32,
    pub max_time: f32,
    pub flags: u32,
    pub break_effect: Option<String>,
    pub break_sound: Option<String>,
    pub break_notify: Option<String>,
    pub loop_sound: Option<String>,
    pub has_phys_preset: bool,
    pub spawn_models: [Option<String>; 3],
}

impl T5DestructiblePiece {
    pub fn stage(&self, index: usize, health: i16) -> Option<usize> {
        for (i, st) in self.stages.iter().enumerate() {
            if health as f32 >= self.health as f32 * st.break_health {
                return st.show_bone.as_ref().map(|_| i);
            }
            if st.show_bone.is_none() {
                return if index == 0 { i.checked_sub(1) } else { None };
            }
        }
        None
    }
}

impl T5DestructibleDef {
    pub fn hide_parts(&self, health: &[i16], bone_names: &[String]) -> crate::HidePartBits {
        assert_eq!(health.len(), self.pieces.len());
        let mut words = [0u32; 6];
        for piece in &self.pieces {
            for (word, hide) in words.iter_mut().zip(piece.hide_bones) {
                *word |= hide;
            }
        }
        for (i, piece) in self.pieces.iter().enumerate() {
            if let Some(st) = piece.stage(i, health[i])
                && let Some(bone) = bone_names
                    .iter()
                    .position(|name| Some(name) == piece.stages[st].show_bone.as_ref())
            {
                words[bone / 32] &= !(0x8000_0000 >> (bone % 32));
            }
        }
        crate::HidePartBits::from_words(words)
    }
}
