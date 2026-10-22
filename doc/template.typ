// Оформление ВКР. Параметры перенесены из «ВКР Лобанов П К 2025 финал.docx»:
// поля 20/15/20/25 мм, Times New Roman 14 pt, интервал 1,5, отступ 1,25 см,
// главы «Глава N.» 18 pt по центру с новой страницы, подписи под объектами.

#let font-main = "Times New Roman"
#let font-mono = ("Liberation Mono", "DejaVu Sans Mono")

// Интервал 1,5 строки Word для Times New Roman: расстояние между базовыми
// линиями ≈ 1,66 em; в Typst к нему прибавляется высота прописных (0,66 em).
#let leading = 1em
// Отбивка между абзацами в docx: 8 pt после абзаца.
#let par-gap = 8pt

#let indent = 1.25cm

// Титульный лист (повторяет структуру титула из docx)
#let title-page(
  title: none,
  author: none,
  group: none,
  course: none,
  supervisor-name: none,
  supervisor-degree: none,
  head-name: none,
  specialization: none,
  city: none,
  year: none,
) = page(footer: none, {
  set par(first-line-indent: 0pt, justify: false, leading: 0.65em, spacing: 0.65em)

  align(center)[
    Министерство науки и высшего образования РФ \
    ФГБОУ ВО «Тверской государственный университет» \
    Математический факультет \
    Специальность 10.05.01 Компьютерная безопасность \
    Специализация «#specialization»
  ]

  v(1fr)
  align(center)[
    ВЫПУСКНАЯ КВАЛИФИКАЦИОННАЯ РАБОТА \
    (ДИПЛОМНАЯ РАБОТА СПЕЦИАЛИСТА)
  ]
  v(1.5em)
  align(center, par(leading: 0.8em, upper(title)))
  v(1fr)

  set par(leading: 0.8em, spacing: 0.8em)
  align(right)[
    Выполнил: \
    Студент #course курса, группа #group \
    #author

    Подпись:\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_
    #v(0.5em)
    Научный руководитель: \
    #supervisor-degree \
    #supervisor-name

    Подпись:\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_
  ]
  v(1fr)
  align(left)[
    Допущен(а) к защите \
    Руководитель ООП: \
    #head-name \
    \_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_ \
    «\_\_\_»\_\_\_\_\_\_\_\_\_ #year г.
  ]
  v(1fr)
  align(center)[#city, #year]
})

// Счётчик приложений: «Приложение 1. Название»
#let appendix-counter = counter("appendix")
#let appendix(title) = {
  appendix-counter.step()
  context heading(
    level: 2,
    numbering: none,
    [Приложение #appendix-counter.get().first(). #title],
  )
}

#let thesis(
  title: none,
  author: none,
  group: none,
  course: 6,
  supervisor-name: none,
  supervisor-degree: none,
  head-name: none,
  specialization: none,
  city: "Тверь",
  year: datetime.today().year(),
  // Рамки вокруг ссылок — для черновой проверки, в чистовике выключены
  highlight-links: false,
  body,
) = {
  set document(title: title, author: author)

  set page(
    paper: "a4",
    margin: (top: 20mm, bottom: 20mm, left: 25mm, right: 15mm),
    footer: context align(center, counter(page).display()),
  )

  set text(font: font-main, size: 14pt, lang: "ru", hyphenate: false)
  set par(
    justify: true,
    leading: leading,
    spacing: leading + par-gap,
    first-line-indent: (amount: indent, all: true),
  )

  // Маркеры списков: в docx «•» и «1.» на 0,63 см, текст на 1,27 см
  set list(indent: 0.63cm, body-indent: 0.4cm)
  set enum(indent: 0.63cm, body-indent: 0.4cm)

  // ---------- Заголовки ----------
  // Главы: «Глава N. Название»; разделы «N.M.» и «N.M.K.»; глубже — без номера
  set heading(numbering: "1.1.")
  show heading.where(level: 4): set heading(numbering: none, outlined: false)

  show heading: it => {
    set par(first-line-indent: 0pt, justify: false)
    set text(size: 14pt, weight: "bold")
    block(
      above: leading + 2 * par-gap,
      below: leading + par-gap,
      sticky: true,
      {
        if it.numbering != none [#counter(heading).display(it.numbering) ]
        it.body
      },
    )
  }

  show heading.where(level: 1): it => {
    pagebreak(weak: true)
    set par(first-line-indent: 0pt, justify: false)
    set text(size: 18pt, weight: "bold")
    block(width: 100%, below: leading + 2 * par-gap, sticky: true, align(center, {
      if it.numbering != none [Глава #counter(heading).display("1"). ]
      it.body
    }))
  }

  // ---------- Оглавление ----------
  show outline: set heading(numbering: none)
  set outline(depth: 3, indent: n => (0cm, 0.5cm, 1cm).at(n, default: 1cm))
  show outline.entry: it => {
    set par(first-line-indent: 0pt)
    if it.level == 1 and it.element.numbering != none {
      link(it.element.location(), it.indented(
        [Глава #numbering("1", ..counter(heading).at(it.element.location())).],
        it.inner(),
      ))
    } else {
      it
    }
  }

  // ---------- Рисунки, таблицы, листинги ----------
  set figure(gap: 0.8em)
  set figure.caption(separator: ". ")
  show figure: set block(above: leading + 2 * par-gap, below: leading + 2 * par-gap)
  show figure.caption: set par(justify: false, first-line-indent: 0pt)
  show figure.where(kind: image): set figure(supplement: [Рисунок])
  show figure.where(kind: raw): set figure(supplement: [Листинг])
  // Название таблицы — над таблицей, со смещением вправо
  show figure.where(kind: table): set figure.caption(position: top)
  // «Таблица N» справа, под ней название по центру
  show figure.where(kind: table): it => {
    show figure.caption: c => {
      align(right, [#c.supplement #context c.counter.display(c.numbering)])
      v(-0.4em)
      align(right, c.body)
    }
    it
  }

  show table: set par(justify: false, first-line-indent: 0pt, leading: 0.65em)
  show table: set text(size: 12pt)
  set table(inset: 6pt, stroke: 0.5pt)

  show raw: set text(font: font-mono, size: 12pt)
  show raw.where(block: true): set par(justify: false, first-line-indent: 0pt, leading: 0.5em)
  show raw.where(block: true): set align(left)
  show raw.where(block: true): block.with(width: 100%, inset: (left: indent))

  // ---------- Формулы ----------
  set math.equation(
    numbering: (..nums) => "(" + nums.pos().map(str).join(".") + ")",
    supplement: [],
  )

  // ---------- Ссылки ----------
  // Сокращения вместо склоняемых слов: «в гл. 3», «см. п. 2.2.1», «на рис. 1»
  show ref: it => {
    let el = it.element
    if el == none { return it }
    let loc = el.location()
    if el.func() == heading and el.numbering != none {
      let n = counter(heading).at(loc).map(str).join(".")
      link(loc, if el.level == 1 [гл.~#n] else [п.~#n])
    } else if el.func() == figure {
      let n = numbering(el.numbering, ..el.counter.at(loc))
      let sup = if el.kind == image [рис.] else if el.kind == table [табл.] else if el.kind == raw [листинг] else [#el.supplement]
      link(loc, [#sup~#n])
    } else if el.func() == math.equation {
      link(loc, numbering(el.numbering, ..counter(math.equation).at(loc)))
    } else {
      it
    }
  }

  show link: it => {
    if not highlight-links { return it }
    set text(bottom-edge: "bounds", top-edge: "bounds")
    box(it, stroke: (if type(it.dest) == label { green } else { rgb("#ff66ff") }) + 1pt)
  }

  title-page(
    title: title,
    author: author,
    group: group,
    course: course,
    supervisor-name: supervisor-name,
    supervisor-degree: supervisor-degree,
    head-name: head-name,
    specialization: specialization,
    city: city,
    year: year,
  )

  {
    show outline: set text(size: 14pt)
    outline(title: [Оглавление])
  }

  body
}
