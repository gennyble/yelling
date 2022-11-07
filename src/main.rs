mod job;
mod warm;

use camino::Utf8PathBuf;
use confindent::Confindent;
use eyre::bail;

use crate::job::Job;

fn main() -> eyre::Result<()> {
	let conf = Confindent::from_file("yelling.conf")?;
	let jobs = conf.children("Job");

	if jobs.is_empty() {
		println!("Didn't find any jobs!");
		return Ok(());
	}

	for job in jobs {
		let job = Job::new(job)?;

		match job {
			Job::Warm(warm) => {
				let mut env = warm::Environment::new(warm);
				env.populate()?;
				env.parse_files()?;
				env.prepare_output()?;
				env.write_files()?;
				env.print();
			}
		}
	}

	Ok(())
}
