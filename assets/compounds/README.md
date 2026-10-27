# Shipped compounds

The compound definitions Imago ships, one `.icd` file each (see
[compounds](../../docs/compounds.md)). The release copies this folder beside the
program as `compounds/`; a build run from the repository finds it here. They
are searched last, after the project's (`$JOB/compounds`), the user's and those
in `XMS_COMPOUNDS`: copy one into a project to change it there.
