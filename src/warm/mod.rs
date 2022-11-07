use camino::Utf8PathBuf;
use eyre::bail;
use quark::{Inline, Link, Parser};
use std::{cell::RefCell, collections::HashMap, ops::Deref};

use crate::job::Warm;

pub struct Environment {
	warm: Warm,
	dirs: Vec<Directory>,
}

impl Environment {
	pub fn new(warm: Warm) -> Self {
		Self { warm, dirs: vec![] }
	}

	/// Traverse the directory tree and gather the contents of the [Warm] job
	pub fn populate(&mut self) -> eyre::Result<()> {
		Self::gather_dir(&self.warm.indir, &mut self.dirs, &self.warm)
	}

	/// Parses every file as quark and generates backlinks
	pub fn parse_files(&mut self) -> eyre::Result<()> {
		for dir in &self.dirs {
			for file in &dir.files {
				self.html(file)?;
			}
		}

		Ok(())
	}

	pub fn print(&self) {
		for dir in &self.dirs {
			println!("{} -> {}", dir.inpath, dir.outpath);
			for file in &dir.files {
				println!("\t{} -> {}", file.inpath, file.outpath);
			}
		}
	}

	fn gather_dir<P: Into<Utf8PathBuf>>(
		dir: P,
		dirs: &mut Vec<Directory>,
		warm: &Warm,
	) -> eyre::Result<()> {
		let idir = dir.into();
		let odir = warm.outdir.join(idir.strip_prefix(&warm.indir)?);
		let mut dir = Directory {
			inpath: idir,
			outpath: odir,
			files: vec![],
		};

		for entry in dir.inpath.read_dir_utf8()? {
			let entry = entry?;
			let meta = entry.metadata()?;

			// Skip "hidden files" to avoid gathering .vscode and .git et al.
			if entry.file_name().starts_with(".") {
				continue;
			}

			if meta.is_file() {
				let ifile = entry.path();
				let mut ofile = dir.outpath.join(entry.file_name());
				ofile.set_extension("html");

				let content = std::fs::read_to_string(ifile)?;
				let mut parser = Parser::new();
				parser.parse(content);

				dir.files.push(File {
					inpath: ifile.to_path_buf(),
					outpath: ofile,
					content: RefCell::new(Some(Content::Quark(parser))),
					backlinks: RefCell::new(vec![]),
				});
			} else if meta.is_dir() {
				Self::gather_dir(entry.path(), dirs, warm)?
			}
		}

		dirs.push(dir);

		Ok(())
	}

	fn html(&self, file: &File) -> eyre::Result<()> {
		let File {
			inpath,
			outpath,
			content,
			backlinks,
		} = file;

		let parser = match content.take() {
			Some(Content::Quark(parser)) => parser,
			_ => unreachable!(),
		};

		let mut ret = String::new();
		for tok in parser.tokens() {
			match tok {
				quark::Token::Header { level, inner } => ret.push_str(&format!(
					"<h{level}>{}</h{level}>",
					self.html_inline(file, &parser.references, inner)?
				)),
				quark::Token::Paragraph { inner } => ret.push_str(&format!(
					"<p>{}</p>",
					self.html_inline(file, &parser.references, inner)?
				)),
				quark::Token::CodeBlock { lang, code } => {
					ret.push_str(&format!("<pre><code>{}</code></pre>", code))
				}
			}
		}

		content.replace(Some(Content::IncompleteHtml(ret)));
		Ok(())
	}

	fn html_inline(
		&self,
		file: &File,
		refs: &HashMap<String, String>,
		inner: &[Inline],
	) -> eyre::Result<String> {
		let mut ret = String::new();
		for tok in inner {
			match tok {
				Inline::Break => ret.push_str("<br>"),
				Inline::Text(txt) => ret.push_str(txt),
				Inline::Code(code) => ret.push_str(&format!("<code>{code}</code>")),
				Inline::Interlink(interlink) => {
					let Link { name, location } = interlink;
					let location = location.trim();

					let matching_files = self.find_shortest_path(location);
					if matching_files.len() > 1 {
						bail!("reflink {location} resolved to multiple files!")
					}

					match matching_files.first() {
						None => {
							eprintln!("interlink {location} didn't match anything!");
							ret.push_str(&format!("{{{interlink}}}"));
						}
						Some(interlinked_file) => {
							let file_relpath = file.outpath.strip_prefix(&self.warm.outdir)?;
							let interlinked_relpath =
								interlinked_file.outpath.strip_prefix(&self.warm.outdir)?;

							let reflink_path =
								relativise_path(&file_relpath, &interlinked_relpath)?;
							let backlink_path =
								relativise_path(&interlinked_relpath, &file_relpath)?;

							{
								let mut bl = interlinked_file.backlinks.borrow_mut();
								bl.push(backlink_path);
							}

							let link_name = name.as_deref().unwrap_or(location);

							ret.push_str(&format!(r#"<a href="{reflink_path}">{link_name}</a>"#));
						}
					}
				}
				Inline::Link(link) => {
					let Link { name, location } = link;
					let name = name.as_ref().unwrap_or(location);

					ret.push_str(&format!(r#"<a href="{location}">{name}</a>"#));
				}
				Inline::ReferenceLink(link) => {
					let Link { name, location } = link;
					let name = name.as_ref().unwrap_or(location);

					let location = location.trim();
					let location = match refs.get(location) {
						Some(location) => location,
						None => {
							eprintln!("Failed to resolve reflink with location: {location}");
							location
						}
					};

					eprintln!("Failed to resolve reflink with location: {location}");
					ret.push_str(&format!(r#"<a href="{location}">{name}</a>"#))
				}
			}
		}

		Ok(ret)
	}

	/// Attempts to resolve reference links. Finds every file that ends with
	/// the reference
	pub fn find_shortest_path(&self, reflink: &str) -> Vec<&File> {
		let mut files = vec![];

		for dir in &self.dirs {
			for file in &dir.files {
				let search = file.inpath.with_extension("");
				if search.ends_with(reflink) {
					files.push(file)
				}
			}
		}

		files
	}
}

pub struct Directory {
	inpath: Utf8PathBuf,
	outpath: Utf8PathBuf,
	files: Vec<File>,
}

pub struct File {
	inpath: Utf8PathBuf,
	outpath: Utf8PathBuf,
	content: RefCell<Option<Content>>,
	backlinks: RefCell<Vec<Utf8PathBuf>>,
}

pub enum Content {
	Quark(Parser),
	IncompleteHtml(String),
}

impl Content {
	pub fn quark(parser: Parser) -> Self {
		Self::Quark(parser)
	}

	pub fn is_quark(&self) -> bool {
		match self {
			Self::Quark(_) => true,
			_ => false,
		}
	}
}

pub fn relativise_path<A: Into<Utf8PathBuf>, B: Into<Utf8PathBuf>>(
	base: A,
	target: B,
) -> eyre::Result<Utf8PathBuf> {
	let mut base = base.into(); //.canonicalize_utf8()?;
	let target = target.into();
	/*let target = target
	.canonicalize_utf8()
	.wrap_err_with(|| format!("Failed to canonicalize target directory: {target}"))?;*/

	if base.is_file() {
		if !base.pop() {
			// base was previously known to be absolute, but we popped and there
			// wasn't a parent. How can that happen?
			unreachable!()
		}
	}

	let mut pop_count = 0;
	loop {
		if target.starts_with(&base) {
			break;
		}

		if !base.pop() {
			// We're at the root, done.
			break;
		} else {
			pop_count += 1;
		}
	}

	let mut backtrack: Utf8PathBuf = std::iter::repeat("../").take(pop_count - 1).collect();
	let target = target.strip_prefix(base)?.to_owned();

	backtrack.push(target);
	Ok(backtrack)
}
