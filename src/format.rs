use std::fmt::Write;

use annotate_snippets::{renderer::DecorStyle, AnnotationKind, Group, Level, Renderer, Snippet};

use crate::{
    grammar::validator::ValidationResult,
    parser::{Node, ParseError},
};

impl<'a> ValidationResult<'a> {
    pub fn write_all(&self, w: &mut impl Write) -> std::result::Result<(), std::fmt::Error> {
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
        let renderer = Renderer::styled().decor_style(DecorStyle::Unicode);
        writeln!(w, "{}", renderer.render(&reports[..]))
    }

    pub fn print_all(&self) -> std::result::Result<(), std::fmt::Error> {
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
    ) -> std::result::Result<(), std::fmt::Error> {
        let (id, header) = self.kind.id_and_header();
        let span = self.location.index..self.location.index + self.location.len;
        let mut snippet = Snippet::source(txt).annotation(
            AnnotationKind::Primary
                .span(span)
                .label(format!("{:?}", self.kind)),
        );
        if let Some(file) = filename {
            snippet = snippet.path(file);
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

#[cfg(test)]
mod tests {
    use crate::{
        api::ext::local,
        grammar::validator::{
            TokenErrors, ValidationError, ValidationErrors, ValidationResult, ValidationWarning,
            ValidationWarnings,
        },
    };

    #[test]
    pub fn validation_result() {
        let mut results = ValidationResult::new();

        results.errors.push(ValidationError {
            kind: ValidationErrors::CantUseVariable(local("myVar")),
            node: None,
        });
        results.errors.push(ValidationError {
            kind: ValidationErrors::CannotGoBackMoreThan { steps: 5, max: 2 },
            node: None,
        });
        results.warnings.push(ValidationWarning {
            node: None,
            kind: ValidationWarnings::UnusualToken("labubu".into(), TokenErrors::TooLong),
        });
        results.warnings.push(ValidationWarning {
            node: None,
            kind: ValidationWarnings::UsedPrint,
        });
    }
}
