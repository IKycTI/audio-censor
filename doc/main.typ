
#set page(
  margin: (
    left: 30mm,
    right: 10mm,
    top: 15mm,
    bottom: 20mm,
  ),

  number-align: center,
  //////// *** Отображение колонтикула *** /////////////
  footer: context {
    if counter(page).get().first() == 1 [
      #let today = datetime.today()
      #align(center)[ Тверь #today.year() ]
    ] else [
      #align(center)[#context here().page()]
    ]
  },
)

#set par(
  first-line-indent: 15mm,
  // leading: 1pt,
  justify: true,
)

#set text(
  size: 14pt,
  lang: "ru",
  font: "Times New Roman",
  // baseline: 0.3pt,
  // tracking: 0.3pt,
  hyphenate: false,
)
#set heading(
  numbering: "1.",
)

#show heading.where(level: 1): it => {
  set text(size: 16pt, weight: "semibold")
  it
}

#show heading.where(level: 2): set text(size: 14pt, weight: "semibold")

// #set math.equation(
//   numbering: (..nums) => "(" + counter(heading).display() + nums.pos().map(str).join(".") + ")",
//   supplement: [],
// )
#set math.equation(
  numbering: (..nums) => "(" + nums.pos().map(str).join(".") + ")",
  supplement: [],
)

#show outline.entry.where(
  level: 1,
): it => {
  if it.element.body != "Введение" {
    v(14pt, weak: true)
    strong(it)
  } else {
    v(16pt, weak: false)
    strong(it)
  }
}

//////// *** Подцсветка ссылки *** /////////////
#show link: this => {
  let show-type = "box" // "box" or "filled", see below
  let label-color = green
  let default-color = rgb("#ff66ff")

  if show-type == "box" {
    if type(this.dest) == label {
      // Make the box bound the entire text:
      set text(bottom-edge: "bounds", top-edge: "bounds")
      box(this, stroke: label-color + 1pt)
    } else {
      set text(bottom-edge: "bounds", top-edge: "bounds")
      box(this, stroke: default-color + 1pt)
    }
  } else if show-type == "filled" {
    if type(this.dest) == label {
      text(this, fill: label-color)
    } else {
      text(this, fill: default-color)
    }
  } else {
    this
  }
}


#let var = 12
#let num_lab = 4
#let discipline = ""
#let theme = "Разработка и исследование методов потокового цензурирования аудиоданных на основе нейросетевых моделей распознавания речи"
#let group = "М-54"
#let editor = "Коновалов Семён Иванович"
#let teacher_prof = "к.ф.-м.н., доцент кафедры КБ и ММУ"
#let teacher_name = "Цирулёва Валентина Михайловна"

//////// *** Начало документа *** /////////////
#align(center, [
  *Министерство науки и высшего образования Российской Федерации* \
  *Федеральное государственное бюджетное образовательное \
  учреждение высшего образования* \ \
  *«ТВЕРСКОЙ ГОСУДАРСТВЕННОЙ УНИВЕРСИТЕТ»* \
  #line(length: 100%) \
  *Математический факультет* \
  *Кафедра компьютерной безопасности и математических методов управления*
])

#align(horizon + center)[
  *ДИПЛОМНАЯ РАБОТА* \
  *Тема: «#theme»*
]

#align(right + bottom, block[
  #set align(left)
  Выполнил: \
  Cтудент группы #group \
  математического факультета \
  специальность \
  10.05.01 «Компьютерная \
  безопасность» \
  #editor \
  Проверила: \
  #teacher_prof \
  #teacher_name \ \ \ \ \ \ \
])

#pagebreak()
#outline(indent: auto)



#include "charts/0_intro.typ"
#include "charts/0_1_lit_review.typ"
#include "charts/1_analyze.typ"
#include "charts/2_math.typ"
#include "charts/3_programm.typ"
#include "charts/4_test_research.typ"
#include "charts/5_conclusion.typ"

#pagebreak()
#bibliography(
  "refs.bib",
  style: "gost-r-705-2008-numeric",
  full: true,
  title: "Список литературы",
)

#include "charts/6_supplement.typ"
