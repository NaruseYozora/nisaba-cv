#let r = json("resume.json")
#let d = r.document
#let p = d.profile
#let s = d.style
#set document(title: r.name, author: p.name)
#set page(paper: "a4", margin: s.marginMm * 1mm, numbering: "1 / 1")
#set text(font: (if s.fontFamily == "resume-serif-sc" { "Resume Serif SC" } else { "Resume Sans SC" }, "Nisaba CJK Fallback"), size: s.fontSize * 1pt, lang: "zh", fill: rgb("263540"), top-edge: "ascender", bottom-edge: "descender")
#set par(leading: (s.lineHeight - 1) * 1em, spacing: 0.6em)
#show regex("[A-Za-z0-9_/@.?&=:%+~#-]{35,}"): it => it.text.clusters().intersperse(sym.zws).join()
#set heading(numbering: none)
#show heading.where(level: 1): it => block(sticky: true, above: s.sectionGapMm * 1mm, below: 2mm,
  stack(dir: ttb, spacing: 1mm,
    text(size: (s.fontSize + 1) * 1pt, weight: 600, fill: rgb(s.accent), it.body),
    line(length: 100%, stroke: 0.5pt + rgb("aebfc1")),
  )
)
#let date(x) = {
  if x == none { "" } else {
    str(x.year) + if x.month == none { "" } else { "." + if x.month < 10 { "0" } else { "" } + str(x.month) }
  }
}
#let dates(x) = {
  let a = date(x.start)
  let b = if x.ongoing { "至今" } else { date(x.end) }
  if a == "" and b == "" { "" } else { a + " — " + b }
}
#let joined(xs) = xs.filter(x => x != "").join(" · ")
#let header = [
  #if p.name != "" { text(size: 22pt, weight: 600, p.name); v(1.5mm) }
  #if p.title != "" { text(size: (s.fontSize + 1) * 1pt, p.title); v(1.5mm) }
  #text(size: (s.fontSize - 1) * 1pt, joined((p.phone, p.email, p.location)))
  #for f in p.at("customFields", default: ()) { par(text(f.label + "：" + f.value)) }
  #for item in p.links { par(link(item.url, text(item.label + "：" + item.url))) }
]
#block(breakable: true)[
  #if r.photoFile != none { grid(columns: (1fr, 25mm), gutter: 4mm, header, image(r.photoFile, width: 25mm, height: 32mm, fit: "contain")) } else { header }
]
#let module-title(kind) = s.moduleTitles.at(kind, default: (education: "教育经历", work: "工作经历", project: "项目经历", skill: "技能", custom: "自定义", summary: "个人简介").at(kind, default: kind))
#if p.summary != "" { heading(level: 1, text(module-title("summary"))); text(p.summary) }
#let show-block(b) = {
  let c = b.content
  if b.pageBreakBefore { pagebreak(weak: true) }
  block(breakable: true, above: 1mm, below: 2mm)[
    #block(sticky: true, below: 1mm)[
      #text(weight: 600, if c.kind == "education" { c.school } else if c.kind == "work" { c.company } else if c.kind == "custom" { c.title } else { c.name })
      #let details = if c.kind == "education" { joined((c.degree, c.major, dates(c.dates))) } else if c.kind == "work" { joined((c.role, c.department, c.location, dates(c.dates))) } else if c.kind == "project" { joined((c.role, dates(c.dates))) } else if c.kind == "custom" { joined((c.subtitle, dates(c.dates))) } else { "" }
      #if details != "" { linebreak(); text(size: (s.fontSize - 1) * 1pt, fill: rgb("50636a"), details) }
    ]
    #if c.kind == "skill" { text(c.description) }
    #if c.kind == "project" {
      if c.background != "" { par(text(c.background)) }
      if c.url != none { par(link(c.url.url, text(if c.url.label.trim() == "" { c.url.url } else { c.url.label.trim() + "：" + c.url.url }))) }
    }
    #for a in b.achievements { block(above: 0.8mm, below: 0.8mm)[#text("• " + a.text)] }
  ]
}
#if d.formatVersion == 2 {
  for section in d.at("sections", default: ()) {
    let blocks = section.blockIds.map(id => d.blocks.find(b => b.id == id)).filter(b => b.visible)
    if blocks.len() > 0 {
      if blocks.first().pageBreakBefore { pagebreak(weak: true) }
      heading(level: 1, text(section.title))
      for (i,b) in blocks.enumerate() { if i == 0 { let first = b; first.pageBreakBefore = false; show-block(first) } else { show-block(b) } }
    }
  }
} else {
  for b in d.blocks {
    if b.visible {
      if b.pageBreakBefore { pagebreak(weak: true) }
      heading(level: 1, text(module-title(b.content.kind)))
      let body = b
      body.pageBreakBefore = false
      show-block(body)
    }
  }
}
