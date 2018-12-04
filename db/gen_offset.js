var OffsetLog = require('flumelog-offset')
var codec = require('flumecodec')
var Flume = require('flumedb')

var db = Flume(OffsetLog("./test.offset", {codec: codec.json}))

const NUM_ELEMENTS = 10

for (var i = 0; i < NUM_ELEMENTS; i++) {
  db.append({value: i}, function (cb) {

  })
}
