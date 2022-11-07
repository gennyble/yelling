use bempline::{Document, Options};
use camino::Utf8PathBuf;
use confindent::Value;
use eyre::bail;

pub enum Job {
	Warm(Warm),
}

impl Job {
	pub fn new(conf: &Value) -> eyre::Result<Job> {
		let kind = conf.child_value("Type");

		match kind.map(|s| s.to_lowercase()).as_deref() {
			None => bail!("No job type"),
			Some("warm") => Ok(Job::Warm(Warm::new(conf)?)),
			Some(kind) => bail!("Job type {kind} not understood"),
		}
	}
}

#[derive(Clone, Debug)]
pub struct Warm {
	pub name: String,
	pub indir: Utf8PathBuf,
	pub outdir: Utf8PathBuf,
	pub template: Document,
	pub content_key: String,
	pub backlink_pattern: String,
	pub backlink_key: String,
	pub backlink_name_key: String,
	pub friend_pattern: String,
	pub friend_key: String,
	pub friend_name_key: String,
}

impl Warm {
	pub fn new(conf: &Value) -> eyre::Result<Warm> {
		let name = conf.value_owned().unwrap();
		let indir = conf.child_parse("In").unwrap();
		let outdir = conf.child_parse("Out").unwrap();

		let ts = conf.child("Template").unwrap();
		let template = Document::from_file(ts.value().unwrap(), Options::default())?;
		let content_key = ts.child_owned("ContentKey").unwrap();
		let backlink_pattern = ts.child_owned("BacklinkPattern").unwrap();
		let backlink_key = ts.child_owned("BacklinkKey").unwrap();
		let backlink_name_key = ts.child_owned("BacklinkNameKey").unwrap();
		let friend_pattern = ts.child_owned("FriendPattern").unwrap();
		let friend_key = ts.child_owned("FriendKey").unwrap();
		let friend_name_key = ts.child_owned("FriendNameKey").unwrap();

		Ok(Self {
			name,
			indir,
			outdir,
			template,
			content_key,
			backlink_pattern,
			backlink_key,
			backlink_name_key,
			friend_pattern,
			friend_key,
			friend_name_key,
		})
	}
}
