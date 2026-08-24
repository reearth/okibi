// The one host global this package touches, declared rather than pulled in
// with a whole runtime's types.
//
// Workers, Node and browsers all have `console`; none of them is what this
// package targets specifically, and taking a dependency on one runtime's
// type package to say so would be a heavier claim than the code makes.
declare const console: { error(...data: unknown[]): void };
