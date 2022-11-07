use std::collections::HashMap;
use std::{collections::VecDeque, fs, io::Write, sync::RwLock};

use camino::{Utf8Path, Utf8PathBuf};
use eyre::bail;
use eyre::WrapErr;
use quark::Link;
use quark::{Inline, Parser};

use crate::job::Job;

pub struct Environment {
	pub job: Job,
	files: VecDeque<File>,
}

impl Environment {
	pub fn new(job: Job) -> Self {
		Self {
			job,
			files: VecDeque::new(),
		}
	}

	pub fn file<P: Into<Utf8PathBuf>, O: Into<Utf8PathBuf>>(
		&mut self,
		inpath: P,
		outpath: O,
	) -> eyre::Result<()> {
		let inpath = inpath.into();
		let outpath = outpath.into();
		let quark_raw = std::fs::read_to_string(&inpath)?;

		let in_relpath = inpath.strip_prefix(&self.job.indir)?.to_owned();
		let out_relpath = outpath.strip_prefix(&self.job.outdir)?.to_owned();

		let mut parser = quark::Parser::new();
		parser.parse(quark_raw);

		self.files.push_back(File {
			inpath,
			outpath: outpath.into(),
			in_relpath,
			out_relpath,
			content: Content::quark(parser),
			backlinks: vec![],
		});

		Ok(())
	}

	pub fn run_files(&mut self) -> eyre::Result<()> {
		loop {
			let mut file = match self.files.pop_front() {
				None => break,
				Some(file) => file,
			};

			if file.content.is_quark() {
				file.content = Content::IncompleteHtml(Self::html(
					&mut file,
					&mut self.files,
					&self.job.outdir,
				)?)
			} else {
				self.files.push_back(file);
				break;
			}

			self.files.push_back(file);
		}

		Ok(())
	}

	pub fn finish(self) -> eyre::Result<()> {
		for mut file in self.files {
			let mut doc = self.job.template.clone();
			match file.content {
				Content::Quark(_) => panic!(),
				Content::IncompleteHtml(html) => {
					println!("Running {}", file.inpath);
					doc.set(&self.job.content_key, html);

					file.backlinks.dedup();

					for bl in file.backlinks {
						let mut pat = doc.get_pattern(&self.job.backlink_pattern).unwrap();
						pat.set(&self.job.backlink_key, &bl);
						pat.set(&self.job.backlink_name_key, bl.file_stem().unwrap());
						doc.set_pattern(&self.job.backlink_pattern, pat);
					}
				}
			}

			let mut htmlfile = fs::File::create(file.outpath)?;
			let html = doc.compile();
			htmlfile.write_all(html.as_bytes())?
		}

		Ok(())
	}

	fn html(
		file: &mut File,
		files: &mut VecDeque<File>,
		outdir: &Utf8Path,
	) -> eyre::Result<String> {
		let File {
			inpath,
			outpath,
			in_relpath,
			out_relpath,
			content,
			backlinks,
		} = file;

		let parser = match content {
			Content::Quark(parser) => parser,
			Content::IncompleteHtml(_) => unreachable!(),
		};

		let mut ret = String::new();
		for tok in parser.tokens() {
			match tok {
				quark::Token::Header { level, inner } => ret.push_str(&format!(
					"<h{level}>{}</h{level}>",
					Self::html_inlines(
						out_relpath.clone(),
						files,
						outdir,
						inner,
						&parser.references
					)?
					.trim()
				)),
				quark::Token::Paragraph { inner } => ret.push_str(&format!(
					"<p>{}</p>",
					Self::html_inlines(
						out_relpath.clone(),
						files,
						outdir,
						inner,
						&parser.references
					)?
				)),
				quark::Token::CodeBlock { lang, code } => {
					ret.push_str(&format!("<pre><code>{}</code></pre>", code))
				}
			}
		}

		Ok(ret)
	}

	fn html_inlines(
		file_relpath: Utf8PathBuf,
		files: &mut VecDeque<File>,
		outdir: &Utf8Path,
		inlines: &[Inline],
		references: &HashMap<String, String>,
	) -> eyre::Result<String> {
		let mut ret = String::new();

		for inl in inlines {
			match inl {
				Inline::Break => ret.push_str("<br>"),
				Inline::Text(str) => ret.push_str(str),
				Inline::Code(code) => ret.push_str(&format!("<code>{code}</code>")),
				Inline::Interlink(interlink) => {
					let Link { name, location } = interlink.clone();

					let location = location.trim();
					let mut matching_files = Self::find_shortest_path(files, &location);
					if matching_files.len() > 1 {
						bail!("reflink {location} resolved to multiple files!")
					}

					match matching_files.first_mut() {
						None => {
							eprintln!(
								"interlink {location} didn't match anything! (for {file_relpath})"
							);
							ret.push_str(&format!("{{{interlink}}}"));
						}
						Some(file) => {
							let reflink_path = relativise_path(&file_relpath, &file.out_relpath)?;
							let backlink_path = relativise_path(&file.out_relpath, &file_relpath)?;
							file.backlinks.push(backlink_path);

							let link_name = name.unwrap_or(location.to_string());

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
					let location = match references.get(location) {
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

	pub fn find_shortest_path<'j>(
		files: &'j mut VecDeque<File>,
		reflink: &str,
	) -> Vec<&'j mut File> {
		files
			.iter_mut()
			.filter(|file| {
				let search = file.inpath.with_extension("");
				search.ends_with(reflink)
			})
			.collect()
	}
}
