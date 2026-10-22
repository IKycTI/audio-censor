#import "template.typ": *

#show: thesis.with(
  title: "Разработка и исследование методов потокового цензурирования аудиоданных на основе нейросетевых моделей распознавания речи",
  author: "Коновалов Семён Иванович",
  group: "М-54",
  course: 6,
  supervisor-degree: "канд. физ.-мат. наук, доцент",
  supervisor-name: "Цирулёва Валентина Михайловна",
  head-name: "Малышкина О.В.",
  specialization: "Математические методы защиты информации",
  // highlight-links: true,
)

#include "charts/0_intro.typ"
// #include "charts/0_1_lit_review.typ"
#include "charts/1_analyze.typ"
#include "charts/2_math.typ"
#include "charts/3_programm.typ"
#include "charts/4_test_research.typ"
#include "charts/5_conclusion.typ"

#bibliography(
  "refs.bib",
  style: "gost-r-705-2008-numeric",
  full: true,
  title: "Список литературы",
)

#include "charts/6_supplement.typ"
