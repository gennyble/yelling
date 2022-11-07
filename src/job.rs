use bempline::{Document, Options};
use camino::Utf8PathBuf;
use confindent::Value;

#[derive(Clone, Debug)]
pub struct Job {
	pub name: String,
	pub indir: Utf8PathBuf,
	pub outdir: Utf8PathBuf,
	pub template: Document,
	pub content_key: String,
	pub backlink_pattern: String,
	pub backlink_key: String,
	pub backlink_name_key: String,
}

impl Job {
	pub fn new(conf: &Value) -> eyre::Result<Job> {
		let name = conf.value_owned().unwrap();
		let indir = conf.child_parse("In").unwrap();
		let outdir = conf.child_parse("Out").unwrap();

		let ts = conf.child("Template").unwrap();
		let template = Document::from_file(ts.value().unwrap(), Options::default())?;
		let content_key = ts.child_owned("ContentKey").unwrap();
		let backlink_pattern = ts.child_owned("BacklinkPattern").unwrap();
		let backlink_key = ts.child_owned("BacklinkKey").unwrap();
		let backlink_name_key = ts.child_owned("BacklinkNameKey").unwrap();

		Ok(Self {
			name,
			indir,
			outdir,
			template,
			content_key,
			backlink_pattern,
			backlink_key,
			backlink_name_key,
		})
	}
}
