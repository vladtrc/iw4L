use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use hud_iw4::{ExprError, ExprHost, MenuParsers, Statement};

/// The menu expression parser of each game whose catalogs tag their programs.
#[derive(Resource, Clone, Default)]
pub struct GameMenuParsers(pub MenuParsers);

#[derive(Resource, Default)]
pub(crate) struct MenuExprCache {
    by_dump: HashMap<String, Result<Statement, ExprError>>,
    parsers: MenuParsers,
}

pub(crate) fn sync_parsers(parsers: Res<GameMenuParsers>, mut cache: ResMut<MenuExprCache>) {
    if parsers.is_changed() {
        cache.parsers = parsers.0.clone();
        cache.by_dump.clear();
    }
}

impl MenuExprCache {
    fn statement(&mut self, dump: &str) -> Result<&Statement, ExprError> {
        if !self.by_dump.contains_key(dump) {
            let statement = Statement::parse_with(dump, &self.parsers);
            self.by_dump.insert(dump.to_owned(), statement);
        }
        match self.by_dump.get(dump) {
            Some(Ok(statement)) => Ok(statement),
            Some(Err(err)) => Err(err.clone()),

            None => Err(ExprError::TruncatedDump),
        }
    }

    pub(crate) fn is_true(&mut self, dump: &str, host: &impl ExprHost) -> Result<bool, ExprError> {
        self.statement(dump)?.is_true(host)
    }

    pub(crate) fn evaluate_float(
        &mut self,
        dump: &str,
        host: &impl ExprHost,
    ) -> Result<f32, ExprError> {
        self.statement(dump)?.evaluate_float(host)
    }

    pub(crate) fn evaluate_string(
        &mut self,
        dump: &str,
        host: &impl ExprHost,
    ) -> Result<String, ExprError> {
        self.statement(dump)?.evaluate_string(host)
    }

    pub(crate) fn evaluate(
        &mut self,
        dump: &str,
        host: &impl ExprHost,
    ) -> Result<hud_iw4::Operand, ExprError> {
        self.statement(dump)?.evaluate(host)
    }
}
