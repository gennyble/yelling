use camino::Utf8PathBuf;
use eyre::bail;
use quark::{Inline, Link, Parser};
use std::{cell::RefCell, collections::HashMap, io::Write};

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

	pub fn prepare_output(&mut self) -> eyre::Result<()> {
		self.dirs.sort_by(|a, b| {
			a.outpath
				.components()
				.count()
				.cmp(&b.outpath.components().count())
		});

		for dir in &self.dirs {
			if dir.outpath == self.warm.outdir {
				continue;
			}

			std::fs::create_dir_all(&dir.outpath)?;
		}

		Ok(())
	}

	pub fn write_files(&mut self) -> eyre::Result<()> {
		for dir in self.dirs.iter_mut() {
			let friends = dir
				.files
				.iter()
				.map(|file| {
					file.outpath
						.strip_prefix(&self.warm.outdir)
						.unwrap()
						.to_path_buf()
				})
				.collect::<Vec<Utf8PathBuf>>();

			for file in dir.files.iter_mut() {
				let mut doc = self.warm.template.clone();
				match file.content.take().unwrap() {
					Content::Quark(_) => panic!(),
					Content::IncompleteHtml(html) => {
						println!("Running {}", file.inpath);
						doc.set(&self.warm.content_key, html);

						let mut backlinks = file.backlinks.borrow_mut();
						backlinks.dedup();

						for bl in backlinks.iter() {
							let mut pat = doc.get_pattern(&self.warm.backlink_pattern).unwrap();
							pat.set(&self.warm.backlink_key, &bl);
							pat.set(&self.warm.backlink_name_key, bl.file_stem().unwrap());
							doc.set_pattern(&self.warm.backlink_pattern, pat);
						}

						let relpath = file.outpath.strip_prefix(&self.warm.outdir)?;
						for fr in friends.iter() {
							if fr == relpath {
								continue;
							}

							let mut name = fr.clone();
							name.set_extension("");

							let mut pat = doc.get_pattern(&self.warm.friend_pattern).unwrap();
							pat.set(&self.warm.friend_key, relativise_path(relpath, fr)?);
							pat.set(
								&self.warm.friend_name_key,
								name.components().last().unwrap(),
							);
							doc.set_pattern(&self.warm.friend_pattern, pat);
						}
					}
				}

				let mut htmlfile = std::fs::File::create(&file.outpath)?;
				let html = doc.compile();
				htmlfile.write_all(html.as_bytes())?
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
		let parser = match file.content.take() {
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

		file.content.replace(Some(Content::IncompleteHtml(ret)));
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
