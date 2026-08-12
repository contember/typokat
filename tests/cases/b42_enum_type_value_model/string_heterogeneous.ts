enum Corpus {
  Classic = "classic",
  Tech = "tech",
  Startup = "startup",
  Template = `template`,
  Concatenated = "con" + "cat",
}

enum OtherCorpus {
  Classic = "classic",
}

enum Mixed {
  Zero,
  Text = "text",
  Two = 2,
}

enum StringAliases {
  First = "same",
  Second = "same",
}

const wholeString: Corpus = Corpus.Classic;
const memberString: Corpus.Classic = Corpus.Classic;
const memberToLiteral: "classic" = Corpus.Classic;
const wholeMixedNumber: Mixed = 0;
const wholeMixedMember: Mixed = Mixed.Text;
const equalStringAlias: StringAliases.First = StringAliases.Second;
declare const wholeCorpus: Corpus;
const wholeStringUnion: "classic" | "tech" | "startup" | "template" | "concat" = wholeCorpus;

const rawStringToWhole: Corpus = "classic"; // error[TK2322]
const rawStringToMember: Corpus.Classic = "classic"; // error[TK2322]
const otherStringEnum: Corpus = OtherCorpus.Classic; // error[TK2322]
const rawMixedString: Mixed = "text"; // error[TK2322]
const unknownMixedNumber: Mixed = 99; // error[TK2322]
