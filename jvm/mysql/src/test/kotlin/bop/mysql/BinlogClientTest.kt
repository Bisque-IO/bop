package bop.mysql

import bop.mysql.binlog.BinlogClient
import kotlin.test.Test

class BinlogClientTest {
   @Test
   fun testBinlogClient() {
      val client = BinlogClient("localhost", 3306, null, "root", "")
      client.connect()
      println("connected")
   }

   /*
   2654724
   2654732
   [ 60, 0, 0, 1, 4, -126, 40, 0, 0, 0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 114, 111, 111, 116, 0, 0, 99, 97, 99, 104, 105, 110, 103, 95, 115, 104, 97, 50, 95, 112, 97, 115, 115, 119, 111, 114, 100, 0 ]

   [ 60, 0, 0, 1, 4, -126, 40, 0, 0, 0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 114, 111, 111, 116, 0, 0, 99, 97, 99, 104, 105, 110, 103, 95, 115, 104, 97, 50, 95, 112, 97, 115, 115, 119, 111, 114, 100, 0 ]
   [ 60, 0, 0, 1, 4, -126, 40, 0, 0, 0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 114, 111, 111, 116, 0, 0, 99, 97, 99, 104, 105, 110, 103, 95, 115, 104, 97, 50, 95, 112, 97, 115, 115, 119, 111, 114, 100, 0 ]
    */

   @Test
   fun testBinlogClient2() {
      val client = MySQLConnection("localhost", 3306, null, "root", "")
      client.connect()
   }
}