# INDEPENDENT ORACLE for the Rust comment lexer.
#
# Pygments' RustLexer is written by different people, in a different language,
# and is not derived from anything in this repo. It decides for every character
# whether it is inside a COMMENT or not. Blank exactly the comment characters
# and the result must equal what blankRustComments(.., {blankLiterals:false})
# produces -- which is the whole claim of the shared .rs path.
import sys, json
from pygments.lexers import RustLexer
from pygments.token import Comment, String

# Pygments' RustLexer classifies Rust DOC comments (`///`, `//!`) as
# String.Doc, not Comment -- verified by inspecting a disagreement rather than
# trusting the count. They are comments for our purpose. Comment.Preproc is
# Rust's `#[...]` ATTRIBUTES, which are real code and must not be blanked.
def is_comment(tok):
    if tok is String.Doc:
        return True
    return tok in Comment and tok is not Comment.Preproc

def blank_comments(src):
    lx = RustLexer(stripnl=False, ensurenl=False)
    out = list(src)
    for idx, tok, val in lx.get_tokens_unprocessed(src):
        if is_comment(tok):
            for k in range(idx, min(idx + len(val), len(src))):
                if out[k] != "\n":
                    out[k] = " "
    return "".join(out)

root = sys.argv[1]
files = [l for l in open(sys.argv[2]).read().split("\n") if l]
res = {}
for rel in files:
    src = open(root + "/" + rel, encoding="utf8").read()
    res[rel] = blank_comments(src)
json.dump(res, open(sys.argv[3], "w"))
print("oracle lexed %d rust files" % len(files))
