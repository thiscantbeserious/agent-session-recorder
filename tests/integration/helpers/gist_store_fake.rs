//! Scripted `gh` runner for the gist store tests, replies pop in order and every call is recorded.

use agr::sync::testing::{Output, Runner};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;

type Files = Vec<(String, Vec<u8>)>;

#[derive(Default)]
pub struct Fake {
    pub calls: RefCell<Vec<String>>,
    pub replies: RefCell<VecDeque<std::io::Result<Output>>>,
    /// Files a `gh api <raw_url>` serves, named like the last url segment.
    pub remote: Files,
    /// A server that ignores `Range` and answers 200 with the whole part.
    pub ignore_range: bool,
}

pub fn ok(stdout: &str) -> std::io::Result<Output> {
    Ok(Output {
        code: Some(0),
        stdout: stdout.into(),
        stderr: String::new(),
    })
}

pub fn fail(stderr: &str) -> std::io::Result<Output> {
    Ok(Output {
        code: Some(1),
        stdout: String::new(),
        stderr: stderr.into(),
    })
}

impl Fake {
    pub fn with(replies: Vec<std::io::Result<Output>>) -> Self {
        Self {
            replies: RefCell::new(replies.into()),
            ..Self::default()
        }
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    /// The `remote` bytes a `gh api [-H "Range: bytes=a-b"] <raw_url>` reads.
    fn raw_body(&self, program: &str, args: &[String]) -> Option<Vec<u8>> {
        let range = args.iter().find_map(|a| a.strip_prefix("Range: bytes="));
        let name = args.last()?.rsplit('/').next()?;
        let (_, body) = self.remote.iter().find(|(n, _)| n == name)?;
        let wanted = match range {
            Some(r) if !self.ignore_range => {
                let (from, to) = r.split_once('-')?;
                body[from.parse::<usize>().ok()?..=to.parse::<usize>().ok()?].to_vec()
            }
            _ => body.clone(),
        };
        (program == "gh").then_some(wanted)
    }
}

impl Runner for &Fake {
    fn run_to_file(
        &self,
        program: &str,
        args: &[String],
        env: &[(&str, String)],
        dest: &Path,
    ) -> std::io::Result<Output> {
        let out = self.run(program, args, None, env)?;
        if let (true, Some(body)) = (out.success(), self.raw_body(program, args)) {
            std::fs::write(dest, body)?;
        }
        Ok(out)
    }

    fn run(
        &self,
        program: &str,
        args: &[String],
        _cwd: Option<&Path>,
        _env: &[(&str, String)],
    ) -> std::io::Result<Output> {
        self.calls
            .borrow_mut()
            .push(format!("{program} {}", args.join(" ")));
        let reply = self.replies.borrow_mut().pop_front();
        reply.unwrap_or_else(|| ok(""))
    }
}
