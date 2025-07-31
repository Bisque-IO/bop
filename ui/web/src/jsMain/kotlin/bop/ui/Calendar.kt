package bop.ui

import js.date.Date
import js.objects.Object
import js.objects.unsafeJso
import lib.dayPicker.DayPicker
import lib.dayPicker.DayPickerProps
import lib.dayPicker.defaultDateLib
import lib.dayPicker.useDayPicker
import lib.lucide.ChevronLeftIcon
import lib.lucide.ChevronRightIcon
import react.*
import react.dom.html.ReactHTML.div
import react.dom.html.ReactHTML.nav
import react.dom.html.ReactHTML.span
import react.dom.html.ReactHTML.table
import web.cssom.px

external interface CalendarProps : DayPickerProps {
   var yearRange: Int?
   var showYearSwitcher: Boolean?
   var monthsClassName: String?
   var monthCaptionClassName: String?
   var weekdaysClassName: String?
   var weekdayClassName: String?
   var monthClassName: String?
   var captionClassName: String?
   var captionLabelClassName: String?
   var buttonNextClassName: String?
   var buttonPreviousClassName: String?
   var navClassName: String?
   var monthGridClassName: String?
   var weekClassName: String?
   var dayClassName: String?
   var dayButtonClassName: String?
   var rangeStartClassName: String?
   var rangeEndClassName: String?
   var selectedClassName: String?
   var todayClassName: String?
   var outsideClassName: String?
   var disabledClassName: String?
   var rangeMiddleClassName: String?
   var hiddenClassName: String?
}

external interface DateRange {
   var from: Date?
   var to: Date?
}

external interface YearRange {
   var from: Int
   var to: Int
}

val Calendar = FC<CalendarProps>("Calendar") { props ->
   val showOutsideDays = props.showOutsideDays ?: true
   val showYearSwitcher = props.showYearSwitcher ?: true

   val yearRange = props.yearRange ?: 8

   val (navView, setNavView) = useState("days")
   val (displayYears, setDisplayYears) = useState(
      useMemo(yearRange) {
         val currentYear = Date().getFullYear()
         unsafeJso<YearRange> {
            from = currentYear - (yearRange / 2 + 1)
            to = currentYear + kotlin.math.ceil(yearRange.toDouble() / 2.0).toInt()
         }
      })


//   console.log("dataLib", defaultDateLib)
   val onNextClick = props.onNextClick
   val onPrevClick = props.onPrevClick
   val startMonth = props.startMonth
   val endMonth = props.endMonth

   val columnsDisplayed = if (navView == "years") 1 else props.numberOfMonths

   val monthsClassName = cn("relative flex", props.monthsClassName)
   val monthCaptionClassName = cn("relative mx-4 flex h-7 items-center justify-center", props.monthCaptionClassName)
   val weekdaysClassName = cn("flex flex-row", props.weekdaysClassName)
   val weekdayClassName = cn("w-8 text-sm font-normal text-muted-foreground", props.weekdayClassName)
   val monthClassName = cn("w-full", props.monthClassName)
   val captionClassName = cn("relative flex items-center justify-center pt-1", props.captionClassName)
   val captionLabelClassName = cn("truncate text-sm font-medium", props.captionLabelClassName)
   val buttonNavClassName = cn(buttonVariants(unsafeJso {
      variant = "outline"
      className = "absolute h-7 w-7 bg-transparent p-0 opacity-50 hover:opacity-100"
   }))
   val buttonNextClassName = cn(buttonNavClassName, "right-0", props.buttonNextClassName)
   val buttonPreviousClassName = cn(buttonNavClassName, "left-0", props.buttonPreviousClassName)
   val navClassName = cn("flex items-start", props.navClassName)
   val monthGridClassName = cn("mx-auto mt-4", props.monthGridClassName)
   val weekClassName = cn("mt-2 flex w-max items-start", props.weekClassName)
   val dayClassName = cn("flex size-8 flex-1 items-center justify-center p-0 text-sm", props.dayClassName)
   val dayButtonClassName = cn(
      buttonVariants(unsafeJso { variant = "ghost" }),
      "size-8 rounded-md p-0 font-normal transition-none aria-selected:opacity-100",
      props.dayButtonClassName
   )
   val buttonRangeClassName =
      "bg-accent [&>button]:bg-primary [&>button]:text-primary-foreground [&>button]:hover:bg-primary [&>button]:hover:text-primary-foreground"
   val rangeStartClassName = cn(buttonRangeClassName, "day-range-start rounded-s-md", props.rangeStartClassName)
   val rangeEndClassName = cn(buttonRangeClassName, "day-range-end rounded-e-md", props.rangeEndClassName)
   val rangeMiddleClassName = cn(
      "bg-accent !text-foreground [&>button]:bg-transparent [&>button]:!text-foreground [&>button]:hover:bg-transparent [&>button]:hover:!text-foreground",
      props.rangeMiddleClassName
   )
   val selectedClassName = cn(
      "[&>button]:bg-primary [&>button]:text-primary-foreground [&>button]:hover:bg-primary [&>button]:hover:text-primary-foreground",
      props.selectedClassName
   )
   val todayClassName = cn("[&>button]:bg-accent [&>button]:text-accent-foreground", props.todayClassName)
   val outsideClassName = cn(
      "day-outside text-muted-foreground opacity-50 aria-selected:bg-accent/50 aria-selected:text-muted-foreground aria-selected:opacity-30",
      props.outsideClassName
   )
   val disabledClassName = cn("text-muted-foreground opacity-50", props.disabledClassName)
   val hiddenClassName = cn("invisible flex-1", props.hiddenClassName)

   val components = props.components ?: js("{}")
   Object.assign(components, unsafeJso<dynamic> {
      this.Chevron = CalendarIcon
      this.Nav = { navProps: CalendarNavProps ->
         createElement(CalendarNav, unsafeJso {
            +navProps
            this.className = navProps.className
            this.displayYears = displayYears
            this.navView = navView
            this.setDisplayYears = setDisplayYears
            this.startMonth = startMonth
            this.endMonth = endMonth
            this.onPrevClick = onPrevClick
            this.onNextClick = onNextClick
         })
      }
      this.CaptionLabel = { captionProps: CalendarCaptionLabelProps ->
         createElement(CalendarCaptionLabel, unsafeJso {
            +captionProps
            this.showYearSwitcher = showYearSwitcher
            this.navView = navView
            this.setNavView = setNavView
            this.displayYears = displayYears
         })
      }
      this.MonthGrid = { monthGridProps: CalendarMonthGridProps ->
         createElement(CalendarMonthGrid, unsafeJso {
            +monthGridProps
            this.displayYears = displayYears
            this.startMonth = startMonth
            this.endMonth = endMonth
            this.navView = navView
            this.setNavView = setNavView
         })
      }
   })

   DayPicker {
      +props
      this.showOutsideDays = showOutsideDays
      className = cn("p-3", props.className)
      style = unsafeJso {
         this.width = (248.8 * (columnsDisplayed ?: 1)).px
      }
      classNames = unsafeJso {
         this.months = monthsClassName
         this.month_caption = monthCaptionClassName
         this.weekdays = weekdaysClassName
         this.weekday = weekdayClassName
         this.month = monthClassName
         this.caption = captionClassName
         this.caption_label = captionLabelClassName
         this.button_next = buttonNextClassName
         this.button_previous = buttonPreviousClassName
         this.nav = navClassName
         this.month_grid = monthGridClassName
         this.week = weekClassName
         this.day = dayClassName
         this.day_button = dayButtonClassName
         this.range_start = rangeStartClassName
         this.range_middle = rangeMiddleClassName
         this.range_end = rangeEndClassName
         this.selected = selectedClassName
         this.today = todayClassName
         this.outside = outsideClassName
         this.disabled = disabledClassName
         this.hidden = hiddenClassName
      }
      this.components = components
      this.numberOfMonths = numberOfMonths
   }
}

private external interface CalendarIconProps : Props {
   var orientation: String?
}

private val CalendarIcon = FC<CalendarIconProps> { props ->
   if (props.orientation == "left") {
      ChevronLeftIcon {
         className = cn("h-4 w-4")
      }
   } else {
      ChevronRightIcon {
         className = cn("h-4 w-4")
      }
   }
}

private external interface CalendarNavProps : DefaultProps {
   var navView: String?
   var startMonth: Date?
   var endMonth: Date?
   var displayYears: YearRange?
   var setDisplayYears: StateSetter<YearRange>?
   var onPrevClick: ((Date) -> Unit)?
   var onNextClick: ((Date) -> Unit)?
}

private val CalendarNav = FC<CalendarNavProps>("CalendarNav") { props ->
   val navView = props.navView ?: ""
   val startMonth = props.startMonth
   val endMonth = props.endMonth
   val displayYears = props.displayYears ?: unsafeJso { from = Date().getFullYear(); to = Date().getFullYear() }
   val setDisplayYears = props.setDisplayYears
   val onPrevClick = props.onPrevClick
   val onNextClick = props.onNextClick

   val ctx = useDayPicker()
   val nextMonth = ctx.nextMonth
   val previousMonth = ctx.previousMonth
   val goToMonth = ctx.goToMonth

   val isPreviousDisabled: Boolean = {
      if (navView == "years") {
         (startMonth != null && defaultDateLib.differenceInCalendarDays(
            Date(displayYears.from - 1, 0, 1), startMonth
         ) < 0) || (endMonth != null && defaultDateLib.differenceInCalendarDays(
            Date(displayYears.from - 1, 0, 1), endMonth
         ) > 0)
      } else {
         previousMonth == null
      }
   }()
   val isNextDisabled: Boolean = {
      if (navView == "years") {
         (startMonth != null && defaultDateLib.differenceInCalendarDays(
            Date(displayYears.to + 1, 0, 1), startMonth
         ) < 0) || (endMonth != null && defaultDateLib.differenceInCalendarDays(
            Date(displayYears.to + 1, 0, 1), endMonth
         ) > 0)
      } else {
         nextMonth == null
      }
   }()

   val handlePreviousClick = useCallback(previousMonth, goToMonth) {
      console.log("handlePreviousClick")
      if (previousMonth == null) return@useCallback
      if (navView == "years") {
         setDisplayYears?.invoke { prev ->
            unsafeJso {
               from = prev.from - (prev.to - prev.from + 1)
               to = prev.to - (prev.to - prev.from + 1)
            }
         }
         onPrevClick?.invoke(
            Date(
               displayYears.from - (displayYears.to - displayYears.from), 0, 1
            )
         )
         return@useCallback
      }

      goToMonth(previousMonth)
      onPrevClick?.invoke(previousMonth)
   }

   val handleNextClick = useCallback(nextMonth, goToMonth) {
      if (nextMonth == null) return@useCallback
      if (navView == "years") {
         setDisplayYears?.invoke { prev ->
            unsafeJso {
               from = prev.from + (prev.to - prev.from + 1)
               to = prev.to + (prev.to - prev.from + 1)
            }
         }
         onNextClick?.invoke(
            Date(
               displayYears.from + (displayYears.to - displayYears.from), 0, 1
            )
         )
         return@useCallback
      }

      goToMonth(nextMonth)
      onNextClick?.invoke(nextMonth)
   }

   nav {
      className = cn("flex items-center", props.className)

      Button {
         variant = "outline"
         className = cn("absolute left-0 z-50 h-7 w-7 bg-transparent p-0 opacity-80 hover:opacity-100")
         tabIndex = if (isPreviousDisabled) null else -1
         disabled = isPreviousDisabled
         ariaLabel =
            if (navView == "years") "Go to the previous ${displayYears.to - displayYears.from + 1} years" else "Go to the Previous Month"
         onClick = { handlePreviousClick() }

         ChevronLeftIcon { className = cn("h-4 w-4") }
      }

      Button {
         variant = "outline"
         className = cn("absolute right-0 z-50 h-7 w-7 bg-transparent p-0 opacity-80 hover:opacity-100")
         tabIndex = if (isNextDisabled) null else -1
         disabled = isNextDisabled
         ariaLabel =
            if (navView == "years") "Go to the next ${displayYears.to - displayYears.from + 1} years" else "Go to the Next Month"
         onClick = { handleNextClick() }

         ChevronRightIcon { className = cn("h-4 w-4") }
      }
   }
}

private external interface CalendarCaptionLabelProps : DefaultProps {
   var showYearSwitcher: Boolean?
   var navView: String?
   var setNavView: StateSetter<String>?
   var displayYears: YearRange?
}

private val CalendarCaptionLabel = FC<CalendarCaptionLabelProps>("CalendarCaptionLabel") { props ->
   if (props.showYearSwitcher == null || props.showYearSwitcher == false) {
      span {
         className = cn("relative flex items-center justify-center pt-1 mb-2", props.className)
//         className = cn("truncate text-sm font-medium", props.className)
         +props.children
      }
   } else {
      Button {
         variant = "ghost"
         className = cn("truncate text-sm font-medium")
         size = "sm"
         onClick = {
//            console.log("CalendarCaptionLabel.onclick", props)
            props.setNavView?.invoke {
               if (it == "days") "years" else "days"
            }
         }
         if (props.navView == "days") {
            +props.children
         } else {
            +"${props.displayYears?.from ?: Date().getFullYear()} - ${props.displayYears?.to ?: Date().getFullYear()}"
         }
      }
   }
}

private external interface CalendarMonthGridProps : DefaultProps {
   var displayYears: YearRange?
   var startMonth: Date?
   var endMonth: Date?
   var navView: String?
   var setNavView: StateSetter<String>?
}

private val CalendarMonthGrid = FC<CalendarMonthGridProps>("CalendarMonthGrid") { props ->
   if (props.navView == "years") {
      CalendarYearGrid {
         +props
         displayYears = props.displayYears
         startMonth = props.startMonth
         endMonth = props.endMonth
         navView = props.navView
         setNavView = props.setNavView
         className = props.className
      }
   } else {
      table {
         className = props.className
         +props.children
      }
   }
}

private external interface CalendarYearGridProps : DefaultProps {
   var displayYears: YearRange?
   var startMonth: Date?
   var endMonth: Date?
   var navView: String?
   var setNavView: StateSetter<String>?
}

private val CalendarYearGrid = FC<CalendarYearGridProps>("CalendarYearGrid") { props ->
   val ctx = useDayPicker()
   val goToMonth = ctx.goToMonth
   val selected = ctx.selected
   val now = Date()
   val displayYears = props.displayYears ?: unsafeJso { from = now.getFullYear(); to = now.getFullYear() }

   div {
      className = cn("grid grid-cols-4 gap-y-2", props.className)
      repeat(displayYears.to - displayYears.from + 1) { index ->
         val isBefore = props.startMonth != null && defaultDateLib.differenceInCalendarDays(
            Date(displayYears.from + index, 11, 31), props.startMonth!!
         ) < 0
         val isAfter = props.endMonth != null && defaultDateLib.differenceInCalendarDays(
            Date(displayYears.from + index, 0, 0), props.endMonth!!
         ) > 0

         val isDisabled = isBefore || isAfter

         Button {
            variant = "ghost"
            key = index.toString()
            className = cn(
               "h-10 w-full my-1 text-sm font-normal text-foreground",
               if (displayYears.from + index == now.getFullYear()) "bg-accent font-medium text-accent-foreground" else ""
            )
//            style = unsafeJso {
//               marginTop = 6.px
//               marginBottom = 6.px
//            }
            onClick = {
               props.setNavView?.invoke("days")
               goToMonth(
                  Date(
                     displayYears.from + index, (selected as Date?)?.getMonth() ?: 0
                  )
               )
            }
            disabled = if (props.navView == "years") isDisabled else null

            +"${displayYears.from + index}"
         }
      }
   }
}