use camino::Utf8PathBuf;
use quark::Parser;

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
		println!("idir = {idir} | warm.indir = {}", warm.indir);
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

				dir.files.push(File {
					inpath: ifile.to_path_buf(),
					outpath: ofile,
					content: Content::Raw(content),
					backlinks: vec![],
				});
			} else if meta.is_dir() {
				Self::gather_dir(entry.path(), dirs, warm)?
			}
		}

		dirs.push(dir);

		Ok(())
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
	content: Content,
	backlinks: Vec<Utf8PathBuf>,
}

pub enum Content {
	Raw(String),
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
