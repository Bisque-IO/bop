package bop.demo

import bop.ui.Calendar
import bop.ui.DateRange
import bop.ui.cn
import js.date.Date
import js.objects.unsafeJso
import lib.dateFns.addDays
import react.FC
import react.dom.html.ReactHTML.div
import react.useState
import kotlin.time.ExperimentalTime
import kotlin.time.toKotlinInstant

@OptIn(ExperimentalTime::class)
val CalendarDemo = FC {
   val (date, setDate) = useState<Date?>(Date())
   val (dateRange, setDateRange) = useState<DateRange?>(unsafeJso<DateRange> {
      this.from = Date(Date().getFullYear(), 0, 12)
      this.to = addDays(Date (Date().getFullYear(), 0, 12), 30)
   })
   val (range, setRange) = useState<DateRange?>(unsafeJso<DateRange> {
      this.from = Date(Date().getFullYear(), 0, 12)
      this.to = addDays(Date (Date().getFullYear(), 0, 12), 30)
   })

   div {
      className = cn("flex flex-col flex-wrap items-start gap-2 @md:flex-row")
      Calendar {
         mode = "single"
//         captionLayout = "label"
//         showYearSwitcher = false
//         navLayout = "after"
         selected = date
         autoFocus = true
//         onSelect = { setDate(it.unsafeCast<Date>()) }
         className = cn("rounded-md border shadow-sm")
      }

//      Calendar {
//         mode = "range"
//         navLayout = "after"
//         defaultMonth = dateRange?.from
//         selected = dateRange
//         onSelect = { setDateRange(it.unsafeCast<DateRange>()) }
//         numberOfMonths = 2
////         disabled = { d: Date -> d.toKotlinInstant() > Date().toKotlinInstant()  || d.toKotlinInstant() < Date("1900-01-01").toKotlinInstant() }
//         className = cn("rounded-md border shadow-sm")
//      }
//
//      Calendar {
//         mode = "range"
//         navLayout = "after"
//         defaultMonth = range?.from
//         selected = range
//         onSelect = { setRange(it.unsafeCast<DateRange>()) }
//         numberOfMonths = 3
//         className = cn("hidden rounded-md border shadow-sm @4xl:flex [&>div]:gap-5")
//      }
   }
}