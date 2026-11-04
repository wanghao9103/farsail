package app.farsail.android

import org.junit.Assert.*
import org.junit.Test

class ViewerGeometryTest {
    @Test fun fittedRotationMapsTouchBackToSourceAndRejectsBands() {
        for ((w,h) in listOf(360f to 640f, 1280f to 720f)) for (turn in 0..3) {
            val g = ViewerGeometry(w,h,582,1280,turn)
            val rw = g.rotatedWidth*g.scale; val rh = g.rotatedHeight*g.scale
            val left = (w-rw)/2; val top = (h-rh)/2
            val p = g.point(left+rw*.25f,top+rh*.75f)!!
            val expected = listOf(.25 to .75,.75 to .75,.75 to .25,.25 to .25)[turn]
            assertEquals(expected.first,p.first,.00001);assertEquals(expected.second,p.second,.00001)
            assertNull(g.point(if(left>1) 0f else w/2,if(left>1) h/2 else 0f))
            val outside = g.point(-100f,-100f,true)!!
            assertTrue(outside.first in 0.0..1.0 && outside.second in 0.0..1.0)
        }
    }
    @Test fun unavailableGeometryCannotProduceInput() {
        assertNull(ViewerGeometry(0f,600f,582,1280,0).point(0f,0f))
        assertNull(ViewerGeometry(360f,600f,0,0,0).point(0f,0f))
        assertNull(ViewerGeometry(360f,600f,582,1280,0).point(Float.NaN,0f))
    }
    @Test fun zoomedAndPannedRotationUsesTheSameInverseAsDrawing() {
        for (turn in 0..3) {
            val g = ViewerGeometry(720f, 1280f, 582, 1280, turn, 2.5f, -83f, 47f)
            val w = g.rotatedWidth * g.scale; val h = g.rotatedHeight * g.scale
            val point = g.point((720f-w)/2-83f+w*.25f, (1280f-h)/2+47f+h*.75f)!!
            val expected = listOf(.25 to .75,.75 to .75,.75 to .25,.25 to .25)[turn]
            assertEquals(expected.first, point.first, .00001)
            assertEquals(expected.second, point.second, .00001)
        }
    }
}
