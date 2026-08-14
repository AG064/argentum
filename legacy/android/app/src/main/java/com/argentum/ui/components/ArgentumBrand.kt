package com.argentum.ui.components

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.argentum.ui.theme.CrimsonRed
import com.argentum.ui.theme.Silver
import com.argentum.ui.theme.SilverLight

/**
 * Argentum brand mark — an interlocking A/V monogram with an animated
 * silver→crimson gradient sweep. Used on the onboarding welcome step
 * and the splash.
 */
@Composable
fun ArgentumLogo(
    modifier: Modifier = Modifier,
    size: Dp = 96.dp,
    strokeWidth: Dp = 4.dp,
) {
    val infinite = rememberInfiniteTransition(label = "argentum-logo")
    val sweep by infinite.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 3200, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "logo-sweep",
    )

    Box(
        modifier = modifier.size(size),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(modifier = Modifier.size(size)) {
            val w = this.size.width
            val h = this.size.height
            val sw = strokeWidth.toPx()

            val gradient = Brush.linearGradient(
                colors = listOf(
                    SilverLight,
                    Silver,
                    CrimsonRed,
                    Silver,
                    SilverLight,
                ),
                start = Offset(sweep * w * 2f - w * 0.5f, 0f),
                end = Offset(sweep * w * 2f + w * 0.5f, h),
            )

            // Outer "A" / mountain shape — a triangle with a horizontal bar
            val a = Path().apply {
                moveTo(w * 0.5f, h * 0.10f)
                lineTo(w * 0.10f, h * 0.90f)
                lineTo(w * 0.30f, h * 0.90f)
                lineTo(w * 0.36f, h * 0.72f)
                lineTo(w * 0.64f, h * 0.72f)
                lineTo(w * 0.70f, h * 0.90f)
                lineTo(w * 0.90f, h * 0.90f)
                close()
            }
            drawPath(
                path = a,
                brush = gradient,
                style = Stroke(width = sw, cap = StrokeCap.Round),
            )

            // Crossbar of the A
            val bar = Path().apply {
                moveTo(w * 0.39f, h * 0.58f)
                lineTo(w * 0.61f, h * 0.58f)
            }
            drawPath(
                path = bar,
                brush = gradient,
                style = Stroke(width = sw, cap = StrokeCap.Round),
            )

            // Soft glow under the mark
            drawCircle(
                color = CrimsonRed.copy(alpha = 0.18f),
                radius = w * 0.32f,
                center = Offset(w * 0.5f, h * 0.95f),
            )
        }
    }
}

/**
 * Animated Argentum wordmark for the onboarding header and about screens.
 */
@Composable
fun ArgentumWordmark(
    modifier: Modifier = Modifier,
    color: Color = MaterialTheme.colorScheme.onBackground,
) {
    val infinite = rememberInfiniteTransition(label = "argentum-wordmark")
    val shimmer by infinite.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 3600, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "wordmark-shimmer",
    )

    Canvas(
        modifier = modifier,
    ) {
        // Letters are drawn as filled rounded rectangles in a compact
        // geometric style; this is a wordmark, not a typeface.
        val unit = size.height * 0.7f
        val stroke = unit * 0.16f
        val gap = unit * 0.12f
        val baseline = size.height * 0.5f
        var x = 0f

        fun drawA() {
            // Left leg
            drawLine(
                color = color,
                start = Offset(x + unit * 0.5f, baseline - unit),
                end = Offset(x, baseline),
                strokeWidth = stroke,
                cap = StrokeCap.Round,
            )
            // Right leg
            drawLine(
                color = color,
                start = Offset(x + unit * 0.5f, baseline - unit),
                end = Offset(x + unit, baseline),
                strokeWidth = stroke,
                cap = StrokeCap.Round,
            )
            // Crossbar
            drawLine(
                color = color,
                start = Offset(x + unit * 0.18f, baseline - unit * 0.4f),
                end = Offset(x + unit * 0.82f, baseline - unit * 0.4f),
                strokeWidth = stroke * 0.7f,
                cap = StrokeCap.Round,
            )
        }

        fun drawR() {
            // Spine
            drawLine(
                color = color,
                start = Offset(x, baseline - unit),
                end = Offset(x, baseline),
                strokeWidth = stroke,
                cap = StrokeCap.Round,
            )
            // Bowl
            drawLine(
                color = color,
                start = Offset(x, baseline - unit),
                end = Offset(x + unit * 0.8f, baseline - unit),
                strokeWidth = stroke,
                cap = StrokeCap.Round,
            )
            drawLine(
                color = color,
                start = Offset(x + unit * 0.8f, baseline - unit),
                end = Offset(x + unit * 0.8f, baseline - unit * 0.5f),
                strokeWidth = stroke,
                cap = StrokeCap.Round,
            )
            drawLine(
                color = color,
                start = Offset(x + unit * 0.8f, baseline - unit * 0.5f),
                end = Offset(x, baseline - unit * 0.5f),
                strokeWidth = stroke,
                cap = StrokeCap.Round,
            )
            // Leg
            drawLine(
                color = color,
                start = Offset(x + unit * 0.3f, baseline - unit * 0.5f),
                end = Offset(x + unit, baseline),
                strokeWidth = stroke,
                cap = StrokeCap.Round,
            )
        }

        fun drawG() {
            // Open ring
            drawArc(
                color = color,
                startAngle = 30f,
                sweepAngle = 300f,
                useCenter = false,
                topLeft = Offset(x, baseline - unit),
                size = androidx.compose.ui.geometry.Size(unit, unit),
                style = Stroke(width = stroke, cap = StrokeCap.Round),
            )
            // Hook
            drawLine(
                color = color,
                start = Offset(x + unit, baseline - unit * 0.5f),
                end = Offset(x + unit, baseline),
                strokeWidth = stroke,
                cap = StrokeCap.Round,
            )
            drawLine(
                color = color,
                start = Offset(x + unit * 0.5f, baseline),
                end = Offset(x + unit, baseline),
                strokeWidth = stroke,
                cap = StrokeCap.Round,
            )
        }

        fun drawE() {
            drawLine(color, Offset(x, baseline - unit), Offset(x, baseline), stroke, cap = StrokeCap.Round)
            drawLine(color, Offset(x, baseline - unit), Offset(x + unit * 0.85f, baseline - unit), stroke, cap = StrokeCap.Round)
            drawLine(color, Offset(x, baseline - unit * 0.5f), Offset(x + unit * 0.7f, baseline - unit * 0.5f), stroke, cap = StrokeCap.Round)
            drawLine(color, Offset(x, baseline), Offset(x + unit * 0.85f, baseline), stroke, cap = StrokeCap.Round)
        }

        fun drawN() {
            drawLine(color, Offset(x, baseline), Offset(x, baseline - unit), stroke, cap = StrokeCap.Round)
            drawLine(color, Offset(x + unit, baseline), Offset(x + unit, baseline - unit), stroke, cap = StrokeCap.Round)
            drawLine(color, Offset(x, baseline - unit), Offset(x + unit, baseline), stroke, cap = StrokeCap.Round)
        }

        fun drawT() {
            drawLine(color, Offset(x, baseline - unit), Offset(x + unit, baseline - unit), stroke, cap = StrokeCap.Round)
            drawLine(color, Offset(x + unit * 0.5f, baseline - unit), Offset(x + unit * 0.5f, baseline), stroke, cap = StrokeCap.Round)
        }

        fun drawU() {
            drawLine(color, Offset(x, baseline - unit), Offset(x, baseline - unit * 0.4f), stroke, cap = StrokeCap.Round)
            drawLine(color, Offset(x + unit, baseline - unit), Offset(x + unit, baseline - unit * 0.4f), stroke, cap = StrokeCap.Round)
            drawArc(
                color = color,
                startAngle = 180f,
                sweepAngle = 180f,
                useCenter = false,
                topLeft = Offset(x, baseline - unit * 0.4f),
                size = androidx.compose.ui.geometry.Size(unit, unit * 0.4f),
                style = Stroke(width = stroke, cap = StrokeCap.Round),
            )
        }

        fun drawM() {
            drawLine(color, Offset(x, baseline), Offset(x, baseline - unit), stroke, cap = StrokeCap.Round)
            drawLine(color, Offset(x + unit, baseline), Offset(x + unit, baseline - unit), stroke, cap = StrokeCap.Round)
            drawLine(color, Offset(x, baseline - unit), Offset(x + unit * 0.5f, baseline - unit * 0.45f), stroke, cap = StrokeCap.Round)
            drawLine(color, Offset(x + unit * 0.5f, baseline - unit * 0.45f), Offset(x + unit, baseline - unit), stroke, cap = StrokeCap.Round)
        }

        // A R G E N T U M
        drawA(); x += unit + gap
        drawR(); x += unit + gap
        drawG(); x += unit + gap
        drawE(); x += unit + gap
        drawN(); x += unit + gap
        drawT(); x += unit + gap
        drawU(); x += unit + gap
        drawM()

        // Animated shimmer overlay — sweep a crimson→silver gradient across
        val shimmerBrush = Brush.linearGradient(
            colors = listOf(
                Color.Transparent,
                CrimsonRed.copy(alpha = 0.25f * shimmer),
                SilverLight.copy(alpha = 0.4f * (1f - shimmer)),
                Color.Transparent,
            ),
            start = Offset(shimmer * size.width * 1.4f - size.width * 0.2f, 0f),
            end = Offset(shimmer * size.width * 1.4f + size.width * 0.2f, size.height),
        )
        drawRect(
            brush = shimmerBrush,
            topLeft = Offset(0f, 0f),
            size = size,
            alpha = 0.6f,
        )
    }
}
