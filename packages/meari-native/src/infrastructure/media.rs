use crate::{application::media::MediaQueryEvaluator, infrastructure::engine::NativeRenderer};
use style::{
    context::QuirksMode,
    media_queries::MediaList,
    parser::ParserContext,
    stylesheets::{CustomMediaEvaluator, Origin, UrlExtraData},
};

impl MediaQueryEvaluator for NativeRenderer {
    fn matches_media(&mut self, query: &str) -> Result<bool, String> {
        let url = UrlExtraData::from(self.document.url().clone());
        let mut context = ParserContext::new(
            Origin::Author,
            &url,
            None,
            style_traits::ParsingMode::DEFAULT,
            QuirksMode::NoQuirks,
            Default::default(),
            None,
            None,
            Default::default(),
        );
        let list = MediaList::parse(&mut context, &mut cssparser::Parser::new(query));
        // CSS error recovery makes invalid/unknown queries non-matching, matching
        // the CSS engine rather than interpreting query text in application code.
        Ok(list.evaluate(
            self.document.stylist_device(),
            QuirksMode::NoQuirks,
            &mut CustomMediaEvaluator::none(),
        ))
    }
}
