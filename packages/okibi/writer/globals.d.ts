// The one host global this package touches, declared rather than pulled in
// with a whole runtime's types.
//
// Workers, Node and browsers all have `console`; none of them is what this
// package targets specifically, and taking a dependency on one runtime's
// type package to say so would be a heavier claim than the code makes.
declare const console: { error(...data: unknown[]): void };

// WHATWG `URL`, for the same reason and on the same terms: Workers, Node and
// browsers all have it, and `siteOf` uses it to reduce a header to an origin
// rather than pattern-matching one out — a header is something a client can
// send anything in, and a parser that refuses is worth more than a regular
// expression that matches something.
declare const URL: {
  new (url: string): { readonly protocol: string; readonly origin: string };
};
