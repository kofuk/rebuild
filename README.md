# Rebuild

This program watch a file and execute given command when the file updated.

## Example

- Execute Python script when it modified.

```shell
$ rebuild foo.py python '{}'
```

- Compile TeX file when it modified.

```shell
$ rebuild foo.tex -- platex -halt-on-error '{}' '&&' dvipdfmx foo.dvi
```

- Compile C source code when it modified and do something on failure.

```shell
$ rebuild foo.c -- gcc -o foo foo.c '||' echo 'COMPILATION FAILURE'
```

- Do something before rebuild.

```shell
$ rebuild foo.c -- echo 'Start' \; gcc -o foo foo.c
```

## License

GNU General Public License version 3 or later.
