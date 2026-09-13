//! 真实错误日志语料（roadmap ④ 错误诊断升级）——**自动生成，勿手改**。
//!
//! 生成脚本：`scripts/gen-log-error-corpus.ps1`；期望值在 `diagnosis_tests.rs` 手写，
//! 避免"实现自己生成期望值"的自证循环。
//!
//! 来源全部是**真实编译产物**：`survey:` 前缀来自 ⑲ 模板编译矩阵（19 个真实模板），
//! `probe:` 前缀来自故意写错的最小文档。每例只截取错误上下文窗口。

/// 一条真实日志片段。
pub struct RealLogCase {
    /// 用例名（与 `diagnosis_tests.rs` 的期望表按名对应）。
    pub name: &'static str,
    /// 来源（便于复核：⑲ 矩阵目录名 / 探针名）。
    pub source: &'static str,
    /// 真实 `.log` 片段。
    pub log: &'static str,
}

/// 真实错误日志语料。
pub static REAL_LOGS: &[RealLogCase] = &[
    RealLogCase {
        name: "aastex-missing-logo",
        source: "⑲:acmart_ACM___xe",
        log: r##"
(Font)              scaled to size 8.0pt on input line 194.

! LaTeX Error: File `acm-jdslogo' not found.

See the LaTeX manual or LaTeX Companion for explanation.
Type  H <return>  for immediate help.
 ...                                              
                                                  
l.194 \maketitle
                
I could not locate the file with any of these extensions:
"##,
    },
    RealLogCase {
        name: "undefined-cs-dtx",
        source: "⑲:bithesis_北理工___xe",
        log: r##"
(./bithesis-doc.tex
LaTeX2e <2025-11-01>
L3 programming layer <2026-01-19>
! Undefined control sequence.
l.1 \DoNotIndex
               {\newenvironment,\@bsphack,\@empty,\@esphack,\sfcode}
The control sequence at the end of the top line
"##,
    },
    RealLogCase {
        name: "engine-fontspec-under-pdftex",
        source: "⑲:cquthesis_重庆大学___pdf",
        log: r##"
(c:/texlive/2026/texmf-dist/tex/latex/fontspec/fontspec.sty
Package: fontspec 2025/09/29 v2.9g Font selection for XeLaTeX and LuaLaTeX


! Fatal Package fontspec Error: The fontspec package requires either XeTeX or
(fontspec)                      LuaTeX.
(fontspec)                      
(fontspec)                      You must change your typesetting engine to,
(fontspec)                      e.g., "xelatex" or "lualatex" instead of
(fontspec)                      "latex" or "pdflatex".

Type <return> to continue.
 ...                                              
                                                  
l.101 \msg_fatal:nn {fontspec} {cannot-use-pdftex}
                                                  

"##,
    },
    RealLogCase {
        name: "siunitx-invalid-number",
        source: "⑲:cquthesis_重庆大学___xe",
        log: r##"

Package natbib Warning: Citation `r4' on page 12 undefined on input line 355.


Package natbib Warning: Citation `r6' on page 12 undefined on input line 355.


Package natbib Warning: Citation `r6' on page 12 undefined on input line 359.


Package natbib Warning: Citation `z1' on page 12 undefined on input line 359.


Package natbib Warning: Citation `z2' on page 12 undefined on input line 359.


Package natbib Warning: Citation `z3' on page 12 undefined on input line 359.


Package natbib Warning: Citation `r7' on page 12 undefined on input line 359.


Package natbib Warning: Citation `r8' on page 12 undefined on input line 359.


Package natbib Warning: Citation `r9' on page 12 undefined on input line 359.


Package natbib Warning: Citation `r10' on page 12 undefined on input line 359.


LaTeX Warning: Reference `equ:chap1:bayes' on page 12 undefined on input line 3
63.

[12]

LaTeX Warning: Reference `eq:chem' on page 13 undefined on input line 412.

[13]

! Package siunitx Error: Invalid number '1.654 x 2.34 x 3.430'.

For immediate help type H <return>.
 ...                                              
                                                  
l.430 ^^I\num{1.654 x 2.34 x 3.430}
                                   \\

"##,
    },
    RealLogCase {
        name: "engine-unicode-math-under-pdftex",
        source: "⑲:fduthesis_复旦___pdf",
        log: r##"
(c:/texlive/2026/texmf-dist/tex/latex/unicode-math/unicode-math.sty
Package: unicode-math 2023/08/13 v0.8r Unicode maths in XeLaTeX and LuaLaTeX


! Package unicode-math Error: Cannot be run with pdftex!
(unicode-math)                Use XeLaTeX or LuaLaTeX instead.

Type <return> to continue.
 ...                                              
                                                  
l.40 ...ror:nn {unicode-math} {unsupported-engine}
                                                  

"##,
    },
    RealLogCase {
        name: "missing-font-source-han",
        source: "⑲:fduthesis_复旦___xe",
        log: r##"
(Font)                  OMX/cmex/m/n --> TU/LibertinusMath-Regular.otf(3)/b/n o
n input line 851.


! Package fontspec Error: 
(fontspec)                The font "SourceHanSerifSC-Regular" cannot be
(fontspec)                found; this may be but usually is not a fontspec
(fontspec)                bug. Either there is a typo in the font name/file,
(fontspec)                the font is not installed (correctly), or there is
(fontspec)                a bug in the underlying font loading engine
(fontspec)                (XeTeX/luaotfload).

For immediate help type H <return>.
 ...                                              
                                                  
l.857 ...dFeatures    = { CharacterWidth = Full }]
                                                  

"##,
    },
    RealLogCase {
        name: "emergency-stop",
        source: "⑲:hithesis_哈工大___pdf",
        log: r##"
This is XeTeX, Version 3.141592653-2.6-0.999998 (TeX Live 2026) (preloaded format=xelatex 2026.8.23)  12 SEP 2026 17:05
entering extended mode
 \write18 enabled.
 Source specials enabled.
 file:line:error style messages enabled.
 %&-line parsing enabled.
**main.tex;cp tmp/main.pdf main.pdf

! Emergency stop.
<*> main.tex;cp 
                tmp/main.pdf main.pdf
*** (job aborted, file error in nonstop mode)

 
Here is how much of TeX's memory you used:
 4 strings out of 468168
 29 string characters out of 5436524
 426411 words of memory out of 5000000
 28829 multiletter control sequences out of 15000+600000
 627721 words of font info for 40 fonts, out of 8000000 for 9000
 1348 hyphenation exceptions out of 8191
 0i,0n,0p,1b,6s stack positions out of 10000i,1000n,20000p,200000b,200000s
No pages of output.

"##,
    },
    RealLogCase {
        name: "missing-ctex-fontset",
        source: "⑲:hithesis_哈工大___xe",
        log: r##"
(c:/texlive/2026/texmf-dist/tex/latex/ctex/ctex-cs4size.clo
File: ctex-cs4size.clo 2022/07/14 v2.5.10 cs4size option (CTEX)
)

! Class ctexbook Error: CTeX fontset `windowsnew' could not be found.
(ctexbook)              Fontset `windows' will be used instead.

For immediate help type H <return>.
 ...                                              
                                                  
l.1678   { \ctex_load_fontset: }
                                

"##,
    },
    RealLogCase {
        name: "option-clash-natbib",
        source: "⑲:hitszthesis_哈工深___xe",
        log: r##"
(c:/texlive/2026/texmf-dist/tex/latex/url/url.sty
\Urlmuskip=\muskip18
Package: url 2013/09/16  ver 3.4  Verb mode for urls, etc.
))

! LaTeX Error: Option clash for package natbib.

See the LaTeX manual or LaTeX Companion for explanation.
Type  H <return>  for immediate help.
 ...                                              
                                                  
l.184 \RequirePackage
                     {subeqnarray}
The package natbib has already been loaded with options:
"##,
    },
    RealLogCase {
        name: "missing-package-relative",
        source: "⑲:llncs_Springer___xe",
        log: r##"
(c:/texlive/2026/texmf-dist/tex/latex/lm/t1lmr.fd
File: t1lmr.fd 2015/05/01 v1.6.1 Font defs for Latin Modern
))

! LaTeX Error: File `../authorarchive.sty' not found.

Type X to quit or <RETURN> to proceed,
or enter new name. (Default extension: sty)

Enter file name: 
! Emergency stop.
<read *> 
         
l.4 \authorsetup
                {^^M
*** (cannot \read from terminal in nonstop modes)
"##,
    },
    RealLogCase {
        name: "needs-platex-format",
        source: "⑲:jsarticle_日文___xe",
        log: r##"
(c:/texlive/2026/texmf-dist/tex/platex/jsclasses/jsarticle.cls

! LaTeX Error: This file needs format `pLaTeX2e'
               but this is `LaTeX2e'.

See the LaTeX manual or LaTeX Companion for explanation.
Type  H <return>  for immediate help.
 ...                                              
                                                  
l.14 \NeedsTeXFormat{pLaTeX2e}
                              
The current input file will not be processed further,
"##,
    },
    RealLogCase {
        name: "missing-class-seuthesis",
        source: "⑲:seuthesis_东南___xe",
        log: r##"
(./sample.tex
LaTeX2e <2025-11-01>
L3 programming layer <2026-01-19>

! LaTeX Error: File `seuthesis.cls' not found.

Type X to quit or <RETURN> to proceed,
or enter new name. (Default extension: cls)

Enter file name: 
! Emergency stop.
<read *> 
         
l.5 ^^M
       
*** (cannot \read from terminal in nonstop modes)
"##,
    },
    RealLogCase {
        name: "engine-shtthesis-only",
        source: "⑲:shtthesis_上科大___pdf",
        log: r##"
(c:/texlive/2026/texmf-dist/tex/generic/iftex/iftex.sty
Package: iftex 2024/12/12 v1.0g TeX engine tests
)

! Class shtthesis Error: shtthesis only works with LuaLaTeX or XeLaTeX.

See the shtthesis class documentation for explanation.
Type  H <return>  for immediate help.
 ...                                              
                                                  
l.38 ...hesis only works with LuaLaTeX or XeLaTeX}
                                                  %
Pass `-pdflua' or `-pdfxe' option to `latexmk' on compilation.
"##,
    },
    RealLogCase {
        name: "missing-package-slashbox",
        source: "⑲:xtuthesis_湘大_ctexbook___xe",
        log: r##"
(c:/texlive/2026/texmf-dist/tex/latex/relsize/relsize.sty
Package: relsize 2013/03/29 ver 4.1
)
\skiptotal=\skip65
\skiplinenumber=\skip66
\skiprule=\skip67
\skiphlne=\skip68
\skiptext=\skip69
\skiplength=\skip70
\algomargin=\skip71
\skipalgocfslide=\skip72
\algowidth=\dimen264
\inoutsize=\dimen265
\inoutindent=\dimen266
\interspacetitleruled=\dimen267
\interspacealgoruled=\dimen268
\interspacetitleboxruled=\dimen269
\algocf@ruledwidth=\skip73
\algocf@inoutbox=\box59
\algocf@inputbox=\box60
\AlCapSkip=\skip74
\AlCapHSkip=\skip75
\algoskipindent=\skip76
\algocf@nlbox=\box61
\algocf@hangingbox=\box62
\algocf@untilbox=\box63
\algocf@skipuntil=\skip77
\algocf@capbox=\box64
\algocf@lcaptionbox=\skip78
\algoheightruledefault=\skip79
\algoheightrule=\skip80
\algotitleheightruledefault=\skip81
\algotitleheightrule=\skip82
\c@algocfline=\count342
\c@algocfproc=\count343
\c@algocf=\count344
\algocf@algoframe=\box65
\algocf@algobox=\box66
)

! LaTeX Error: File `slashbox.sty' not found.

Type X to quit or <RETURN> to proceed,
or enter new name. (Default extension: sty)

Enter file name: 
! Emergency stop.
<read *> 
         
l.19 \RequirePackage
                    {cite}^^M
*** (cannot \read from terminal in nonstop modes)
"##,
    },
    RealLogCase {
        name: "unclosed-brace",
        source: "probe:unclosed-brace",
        log: r##"
(tmp/main.aux)
\openout1 = `main.aux'.

LaTeX Font Info:    Checking defaults for OML/cmm/m/it on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OMS/cmsy/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OT1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for T1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for TS1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for TU/lmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OMX/cmex/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for U/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
)
Runaway argument?
{unclosed \end {document} 
! File ended while scanning use of \textbf .
<inserted text> 
                \par 
<*> main.tex
            
I suspect you have forgotten a `}', causing me
to read past where you wanted me to stop.
I'll try to recover; but if the error is serious,
you'd better type `E' or `X' now and fix your file.

! Emergency stop.
<*> main.tex
            
*** (job aborted, no legal \end found)

 
Here is how much of TeX's memory you used:
 432 strings out of 468168
 8251 string characters out of 5436524
 426411 words of memory out of 5000000
 29240 multiletter control sequences out of 15000+600000
 627729 words of font info for 41 fonts, out of 8000000 for 9000
 1348 hyphenation exceptions out of 8191
 35i,0n,38p,145b,40s stack positions out of 10000i,1000n,20000p,200000b,200000s
No pages of output.

"##,
    },
    RealLogCase {
        name: "runaway-argument",
        source: "probe:runaway-argument",
        log: r##"
(tmp/main.aux)
\openout1 = `main.aux'.

LaTeX Font Info:    Checking defaults for OML/cmm/m/it on input line 3.
LaTeX Font Info:    ... okay on input line 3.
LaTeX Font Info:    Checking defaults for OMS/cmsy/m/n on input line 3.
LaTeX Font Info:    ... okay on input line 3.
LaTeX Font Info:    Checking defaults for OT1/cmr/m/n on input line 3.
LaTeX Font Info:    ... okay on input line 3.
LaTeX Font Info:    Checking defaults for T1/cmr/m/n on input line 3.
LaTeX Font Info:    ... okay on input line 3.
LaTeX Font Info:    Checking defaults for TS1/cmr/m/n on input line 3.
LaTeX Font Info:    ... okay on input line 3.
LaTeX Font Info:    Checking defaults for TU/lmr/m/n on input line 3.
LaTeX Font Info:    ... okay on input line 3.
LaTeX Font Info:    Checking defaults for OMX/cmex/m/n on input line 3.
LaTeX Font Info:    ... okay on input line 3.
LaTeX Font Info:    Checking defaults for U/cmr/m/n on input line 3.
LaTeX Font Info:    ... okay on input line 3.
)
Runaway argument?
{unclosed \end {document} 
! File ended while scanning use of \foo.
<inserted text> 
                \par 
<*> main.tex
            
I suspect you have forgotten a `}', causing me
to read past where you wanted me to stop.
I'll try to recover; but if the error is serious,
you'd better type `E' or `X' now and fix your file.

! Emergency stop.
<*> main.tex
            
*** (job aborted, no legal \end found)

 
Here is how much of TeX's memory you used:
 433 strings out of 468168
 8254 string characters out of 5436524
 426411 words of memory out of 5000000
 29241 multiletter control sequences out of 15000+600000
 627729 words of font info for 41 fonts, out of 8000000 for 9000
 1348 hyphenation exceptions out of 8191
 35i,0n,38p,147b,40s stack positions out of 10000i,1000n,20000p,200000b,200000s
No pages of output.

"##,
    },
    RealLogCase {
        name: "missing-math-mode",
        source: "probe:missing-math-mode",
        log: r##"
(tmp/main.aux)
\openout1 = `main.aux'.

LaTeX Font Info:    Checking defaults for OML/cmm/m/it on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OMS/cmsy/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OT1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for T1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for TS1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for TU/lmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OMX/cmex/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for U/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.

! Missing $ inserted.
<inserted text> 
                $
l.3 The value is x_
                   1 and y^2.
I've inserted a begin-math/end-math symbol since I think
"##,
    },
    RealLogCase {
        name: "too-many-braces",
        source: "probe:too-many-braces",
        log: r##"
(tmp/main.aux)
\openout1 = `main.aux'.

LaTeX Font Info:    Checking defaults for OML/cmm/m/it on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OMS/cmsy/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OT1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for T1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for TS1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for TU/lmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OMX/cmex/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for U/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.

! Too many }'s.
l.3 Text}
          more
You've closed more groups than you opened.
"##,
    },
    RealLogCase {
        name: "end-without-begin",
        source: "probe:end-without-begin",
        log: r##"
(c:/texlive/2026/texmf-dist/tex/latex/base/size10.clo
File: size10.clo 2025/01/22 v1.4n Standard LaTeX file (size option)
)
\c@part=\count271
\c@section=\count272
\c@subsection=\count273
\c@subsubsection=\count274
\c@paragraph=\count275
\c@subparagraph=\count276
\c@figure=\count277
\c@table=\count278
\abovecaptionskip=\skip49
\belowcaptionskip=\skip50
\bibindent=\dimen148
)

! LaTeX Error: Missing \begin{document}.

See the LaTeX manual or LaTeX Companion for explanation.
Type  H <return>  for immediate help.
 ...                                              
                                                  
l.2 H
     ello
You're in trouble here.  Try typing  <return>  to proceed.
"##,
    },
    RealLogCase {
        name: "double-subscript",
        source: "probe:double-subscript",
        log: r##"
(Font)              <5> on input line 3.

! Double subscript.
l.3 $x_a_
         b$
I treat `x_1_2' essentially like `x_1{}_2'.
"##,
    },
    RealLogCase {
        name: "misplaced-alignment",
        source: "probe:misplaced-alignment",
        log: r##"
(Font)              <5> on input line 3.

! Misplaced alignment tab character &.
l.5 outside &
              here
I can't figure out why you would want to use a tab mark
"##,
    },
    RealLogCase {
        name: "file-ended-scanning",
        source: "probe:file-ended-scanning",
        log: r##"
(tmp/main.aux)
\openout1 = `main.aux'.

LaTeX Font Info:    Checking defaults for OML/cmm/m/it on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OMS/cmsy/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OT1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for T1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for TS1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for TU/lmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OMX/cmex/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for U/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
)
! Emergency stop.
<*> main.tex
            
*** (job aborted, no legal \end found)

 
Here is how much of TeX's memory you used:
 432 strings out of 468168
 8251 string characters out of 5436524
 426411 words of memory out of 5000000
 29240 multiletter control sequences out of 15000+600000
 627729 words of font info for 41 fonts, out of 8000000 for 9000
 1348 hyphenation exceptions out of 8191
 35i,0n,38p,145b,40s stack positions out of 10000i,1000n,20000p,200000b,200000s
No pages of output.

"##,
    },
    RealLogCase {
        name: "non-utf8-source",
        source: "probe:non-utf8-source(gbk+pdflatex)",
        log: r##"
(tmp/main.aux)
\openout1 = `main.aux'.

LaTeX Font Info:    Checking defaults for OML/cmm/m/it on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OMS/cmsy/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OT1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for T1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for TS1/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for OMX/cmex/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.
LaTeX Font Info:    Checking defaults for U/cmr/m/n on input line 2.
LaTeX Font Info:    ... okay on input line 2.


! LaTeX Error: Invalid UTF-8 byte sequence (��).

See the LaTeX manual or LaTeX Companion for explanation.
Type  H <return>  for immediate help.
 ...                                              
                                                  
l.3 ��
      ��GBK��������
The document does not appear to be in UTF-8 encoding.
"##,
    },
];
