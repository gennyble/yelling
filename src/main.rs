mod environment;
mod job;

use camino::Utf8PathBuf;
use confindent::Confindent;
use environment::Environment;

use crate::job::Job;

fn main() -> eyre::Result<()> {
	let conf = Confindent::from_file("yelling.conf")?;
	let job = Job::new(conf.child("Job").unwrap())?;

	let mut env = Environment::new(job.clone());
	run_dir(&mut env, job.indir.clone(), job.outdir.clone())?;
	env.run_files()?;
	env.finish()?;

	Ok(())
}

fn run_dir(env: &mut Environment, indir: Utf8PathBuf, outdir: Utf8PathBuf) -> eyre::Result<()> {
	for entry in indir.read_dir_utf8()? {
		let entry = entry?;
		let meta = entry.metadata()?;

		if entry.file_name().starts_with(".") {
			println!("Skipping {}", entry.file_name());
			continue;
		}

		if meta.is_file() {
			let ifile = entry.path();
			let mut ofile = outdir.join(entry.file_name());
			ofile.set_extension("html");

			env.file(ifile, ofile)?;
			println!("pushed {ifile}");
		} else if meta.is_dir() {
			let outdir = outdir.join(entry.file_name());
			if !outdir.exists() {
				std::fs::create_dir(&outdir)?;
				println!("Created {outdir}");
			}

			run_dir(env, entry.path().to_owned(), outdir)?
		}
	}

	Ok(())
}
