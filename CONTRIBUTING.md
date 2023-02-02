<!--
SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Contributing

These are the contribution guidelines for caterpillar.

Development takes place on GitHub at https://github.com/dvzrv/caterpillar.

## Writing code

This project is written in [Rust](https://www.rust-lang.org/) and formatted using the default [rustfmt](https://github.com/rust-lang/rustfmt) of the most recent version of Rust.

All contributions are linted using [clippy](https://github.com/rust-lang/rust-clippy) and spell checked using [codespell](https://github.com/codespell-project/codespell).

To aide in development, it is encouraged to use the local `.ci/check.sh` script as [git pre-commit hook](https://man.archlinux.org/man/githooks.5#pre-commit):

```shell
$ ln -f -s ../../.ci/check.sh .git/hooks/pre-commit
```

## Writing commit messages

The commit message style follows [conventional commits](https://www.conventionalcommits.org/en/v1.0.0/).

To aide in development, install [cocogitto](https://github.com/cocogitto/cocogitto) and add its [git-hooks](https://man.archlinux.org/man/githooks.5):

```shell
$ cog install-hook all
```

## Creating releases

Releases are created by the developers of this project using [cocogitto](https://github.com/cocogitto/cocogitto) by running:

```shell
cog bump --auto
```

The above will automatically bump the version based on the commits since the last release, add an entry to `CHANGELOG.md`, create a tag, push the changes to the default branch of the project and create a release on https://crates.io.

## License

All code contributions are dual-licensed under the terms of the [Apache-2.0](https://spdx.org/licenses/Apache-2.0.html) and [MIT](https://spdx.org/licenses/MIT.html).

All test environment scripts are licensed under the terms of the [LGPL-3.0-or-later](https://spdx.org/licenses/LGPL-3.0-or-later.html).

All documentation contributions fall under the terms of the [CC-BY-SA-4.0](https://creativecommons.org/licenses/by-sa/4.0/).

All configuration file contributions fall under the terms of the [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).

License identifiers and copyright statements are checked using [reuse](https://git.fsfe.org/reuse/tool). By using `.ci/check.sh` as git pre-commit hook, it is run automatically.
