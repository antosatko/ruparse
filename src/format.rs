use std::{borrow::Cow, fmt::Write};

use annotate_snippets::{renderer::DecorStyle, AnnotationKind, Group, Level, Renderer, Snippet};

use crate::{
    grammar::validator::ValidationResult,
    lexer::PreprocessorError,
    parser::{Node, ParseError},
};

const TERM_WIDTH: usize = 60;

impl<'a> ValidationResult<'a> {
    pub fn write_all(&self, w: &mut impl Write) -> std::fmt::Result {
        let mut reports = Vec::new();
        for warn in &self.warnings {
            let (id, header) = warn.kind.id_and_header();
            let report = Group::with_title(
                Level::WARNING
                    .with_name("parser warning")
                    .primary_title(format!("{}\n{}", header, warn))
                    .id(id),
            );
            reports.push(report);
        }
        for err in &self.errors {
            let (id, header) = err.kind.id_and_header();
            let report = Group::with_title(
                Level::ERROR
                    .with_name("parser erorr")
                    .primary_title(format!("{}\n{}", header, err))
                    .id(id),
            );
            reports.push(report);
        }
        let renderer = Renderer::styled()
            .term_width(TERM_WIDTH)
            .decor_style(DecorStyle::Unicode);
        writeln!(w, "{}", renderer.render(&reports[..]))
    }

    pub fn print_all(&self) -> std::fmt::Result {
        let mut buf = String::new();
        self.write_all(&mut buf)?;
        print!("{buf}");
        Ok(())
    }
}

impl<'a> ParseError<'a> {
    pub fn write(
        &self,
        w: &mut impl Write,
        txt: &'a str,
        filename: Option<&str>,
    ) -> std::fmt::Result {
        let (id, header) = self.kind.id_and_header();
        let span = self.location.index..self.location.index + self.location.len;
        let mut snippet = Snippet::source(txt)
            .annotation(
                AnnotationKind::Primary
                    .span(span)
                    .label(format!("{:?}", self.kind)),
            )
            // .annotation(
            //     AnnotationKind::Visible
            //         .span(self.location.index - 5..self.location.index + self.location.len),
            // )
            .fold(true);
        if let Some(file) = filename {
            snippet = snippet.path(file);
        }
        let header: Cow<'a, str> = match &self.node {
            Some(n) => format!("{header} while parsing {}", n.name).into(),
            None => header.into(),
        };
        match &self.node {
            Some(n) => {
                snippet = snippet.annotation(
                    AnnotationKind::Visible.span(n.first_string_idx..n.first_string_idx + 1),
                )
            }
            _ => (),
        }
        let mut report = Group::with_title(
            Level::ERROR
                .with_name("syntax error")
                .primary_title(header)
                .id(id),
        )
        .element(snippet);
        report = match &self {
            Self {
                hint: Some(hint), ..
            } => report.element(Level::HELP.message(*hint)),
            Self {
                node: Some(Node { docs: Some(d), .. }),
                ..
            } => report.element(Level::INFO.message(*d)),
            _ => report,
        };
        // // if let Some(Node { docs: Some(d), .. }) = &self.node {
        // //     report = report.element(Level::INFO.message(*d));
        // // }
        // if let Some(hint) = self.hint {
        //     report = report.element(Level::HELP.message(hint));
        // }
        let render = Renderer::styled()
            .decor_style(DecorStyle::Unicode)
            .term_width(TERM_WIDTH)
            .render(&[report]);
        write!(w, "{render}")
    }

    pub fn print(&self, txt: &'a str, filename: Option<&str>) -> std::fmt::Result {
        let mut buf = String::new();
        self.write(&mut buf, txt, filename)?;
        println!("{buf}");
        Ok(())
    }
}

impl<'a> PreprocessorError<'a> {
    pub fn write(
        &self,
        w: &mut impl Write,
        txt: &'a str,
        filename: Option<&str>,
    ) -> std::fmt::Result {
        let span = self.location.index..self.location.index + self.len;
        let mut snippet = Snippet::source(txt)
            .annotation(
                AnnotationKind::Primary
                    .span(span)
                    .label(format!("{:?}", self.err.msg)),
            )
            .fold(true);
        if let Some(file) = filename {
            snippet = snippet.path(file);
        }
        let report = Group::with_title(
            Level::ERROR
                .with_name("lexing error")
                .primary_title(self.err.header)
                .id(self.err.code),
        )
        .element(snippet);

        let render = Renderer::styled()
            .decor_style(DecorStyle::Unicode)
            .term_width(TERM_WIDTH)
            .render(&[report]);
        write!(w, "{render}")
    }

    pub fn print(&self, txt: &'a str, filename: Option<&str>) -> std::fmt::Result {
        let mut buf = String::new();
        self.write(&mut buf, txt, filename)?;
        println!("{buf}");
        Ok(())
    }
}
