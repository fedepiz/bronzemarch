use crate::simulation::*;

use num_enum::TryFromPrimitive;
use slotmap::*;
use std::collections::*;
use strum::{EnumCount, EnumIter};
use util::arena::*;

new_key_type! { pub(crate) struct TokenTypeId; }
new_key_type! { pub(crate) struct TokenContainerId; }
new_key_type! { pub(crate) struct TokenId; }

// TOKEN CATEGORY
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumIter, EnumCount, Debug)]
#[repr(usize)]
#[derive(TryFromPrimitive)]
pub(crate) enum TokenCategory {
    Building,
    Pop,
}

pub(crate) struct TokenType {
    pub tag: &'static str,
    pub name: &'static str,
    pub category: TokenCategory,
    pub demand: SecondaryMap<GoodId, f64>,
    pub supply: SecondaryMap<GoodId, f64>,
    pub rgo_points: f64,
}

impl Tagged for TokenType {
    fn tag(&self) -> &str {
        self.tag
    }
}

pub(crate) struct TokenData {
    pub container: TokenContainerId,
    pub typ: TokenTypeId,
    pub size: i64,
}

pub(crate) struct ReadToken<'a> {
    pub id: TokenId,
    pub data: &'a TokenData,
    pub typ: &'a TokenType,
}

impl<'a> ArenaSafe for ReadToken<'a> {}

#[derive(Default)]
pub(crate) struct Tokens {
    pub types: SlotMap<TokenTypeId, TokenType>,
    pub containers: SlotMap<TokenContainerId, BTreeSet<TokenId>>,
    pub tokens: SlotMap<TokenId, TokenData>,
}

impl Tokens {
    pub fn define_type(&mut self, typ: TokenType) -> TokenTypeId {
        match self.types.lookup(typ.tag) {
            Some(existing) => {
                println!("Redefition of token type with tag '{}'", typ.tag);
                existing
            }
            None => self.types.insert(typ),
        }
    }

    pub fn add_container(&mut self) -> TokenContainerId {
        self.containers.insert(Default::default())
    }

    pub fn add_token(
        &mut self,
        container: TokenContainerId,
        typ: TokenTypeId,
        size: i64,
    ) -> TokenId {
        match self.find_token_with_characteristics(container, typ) {
            Some(tok_id) => {
                self.tokens[tok_id].size += size;
                tok_id
            }
            None => {
                let id = self.tokens.insert(TokenData {
                    container,
                    typ,
                    size,
                });
                self.containers[container].insert(id);
                id
            }
        }
    }

    pub fn all_tokens_of_category<'a>(
        &'a self,
        container: TokenContainerId,
        category: TokenCategory,
    ) -> impl Iterator<Item = ReadToken<'a>> + use<'a> {
        self.all_tokens_in(container)
            .filter(move |tok| tok.typ.category == category)
    }

    pub fn all_tokens_in<'a>(
        &'a self,
        container: TokenContainerId,
    ) -> impl Iterator<Item = ReadToken<'a>> {
        self.containers
            .get(container)
            .into_iter()
            .flat_map(|container| container.iter().copied())
            .map(|id| {
                let data = &self.tokens[id];
                let typ = &self.types[data.typ];
                ReadToken { id, data, typ }
            })
    }

    pub fn find_token_with_characteristics(
        &self,
        container: TokenContainerId,
        typ: TokenTypeId,
    ) -> Option<TokenId> {
        self.all_tokens_in(container)
            .find(|tok| tok.data.typ == typ)
            .map(|tok| tok.id)
    }

    pub fn count_size(tokens: &[ReadToken], category: TokenCategory) -> i64 {
        tokens
            .iter()
            .filter(|tok| tok.typ.category == category)
            .map(|tok| tok.data.size)
            .sum()
    }

    pub fn despawn(&mut self, id: TokenContainerId) {
        if let Some(container) = self.containers.remove(id) {
            for id in container {
                self.tokens.remove(id);
            }
        }
    }
}
