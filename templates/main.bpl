<html>
<head>
	<link rel="stylesheet" href="/style.css" />
</head>
<body>
	<nav>
		<a id="home" href="/">nyble.dev</a>
	</nav>
	<main>
	{content}
	</main>
	<section id="backlinks">
	<h3>backlinks</h3>
		<ul>
		{%pattern backlink}
		<a href="{backlink}">{backlink_name}</a>
		{%end}
		</ul>
	</section>
</body>
</html>