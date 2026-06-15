package com.argentum.ui.components

import android.os.Build
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RenderEffect
import androidx.compose.ui.graphics.ShaderBrush
import androidx.compose.ui.graphics.asComposeRenderEffect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.argentum.ui.theme.CrimsonRed
import com.argentum.ui.theme.GlassHighlight
import com.argentum.ui.theme.GlassSilver
import com.argentum.ui.theme.Silver
import com.argentum.ui.theme.SilverLight

/**
 * Argentum "liquid-glass" surface.
 *
 * On API 31+ the surface is blurred with [RenderEffect.createBlurEffect], which
 * gives a real iOS-26-style glass refraction. On older API levels we fall back
 * to a translucent gradient — still recognisably glass, just no blur.
 *
 * Use [GlassCard] for content tiles, [GlassSurface] for full-screen backdrops,
 * and [GlassButton] for tappable surfaces.
 */
@Composable
fun GlassCard(
    modifier: Modifier = Modifier,
    cornerRadius: Dp = 20.dp,
    backgroundColor: Color = MaterialTheme.colorScheme.surface,
    borderColor: Color = SilverLight.copy(alpha = 0.35f),
    blurRadius: Dp = 18.dp,
    content: @Composable () -> Unit
) {
    val shape = RoundedCornerShape(cornerRadius)

    Box(
        modifier = modifier
            .graphicsLayer {
                shape = shape
                clip = true
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    renderEffect = RenderEffect
                        .createBlurEffect(
                            blurRadius.value * density,
                            blurRadius.value * density,
                            Shader.TileMode.CLAMP,
                        )
                        .asComposeRenderEffect()
                }
            }
            .clip(shape)
            .background(
                brush = Brush.verticalGradient(
                    colors = listOf(
                        backgroundColor.copy(alpha = 0.85f),
                        backgroundColor.copy(alpha = 0.55f),
                    )
                )
            )
            .border(
                width = 1.dp,
                brush = Brush.verticalGradient(
                    colors = listOf(
                        SilverLight.copy(alpha = 0.55f),
                        Silver.copy(alpha = 0.15f),
                    )
                ),
                shape = shape
            )
    ) {
        Box(modifier = Modifier.padding(1.dp)) {
            content()
        }
    }
}

/**
 * A full-bleed glass surface — use this for screen backdrops and the
 * onboarding hero region. Animates a slow silver highlight sweep on top of
 * the gradient, which is what gives the surface its "liquid" feel.
 */
@Composable
fun GlassSurface(
    modifier: Modifier = Modifier,
    blurRadius: Dp = 24.dp,
    content: @Composable () -> Unit
) {
    val infinite = rememberInfiniteTransition(label = "glass-sweep")
    val sweep by infinite.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 4200, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "sweep-progress",
    )

    Box(
        modifier = modifier
            .graphicsLayer {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    renderEffect = RenderEffect
                        .createBlurEffect(
                            blurRadius.value * density,
                            blurRadius.value * density,
                            Shader.TileMode.CLAMP,
                        )
                        .asComposeRenderEffect()
                }
            }
            .background(
                brush = Brush.verticalGradient(
                    colors = listOf(
                        MaterialTheme.colorScheme.background,
                        MaterialTheme.colorScheme.surface,
                    )
                )
            )
    ) {
        // Animated silver sweep overlay — gives the surface its liquid shimmer.
        Box(
            modifier = Modifier
                .matchParentSize()
                .background(
                    brush = Brush.linearGradient(
                        colors = listOf(
                            Color.Transparent,
                            SilverLight.copy(alpha = 0.10f),
                            GlassSilver.copy(alpha = 0.18f),
                            SilverLight.copy(alpha = 0.10f),
                            Color.Transparent,
                        ),
                        start = androidx.compose.ui.geometry.Offset(sweep * 1200f - 200f, 0f),
                        end = androidx.compose.ui.geometry.Offset(sweep * 1200f + 200f, 1000f),
                    )
                )
        )
        content()
    }
}

/**
 * A tappable glass button with a subtle red-crimson glow on press. Used
 * everywhere a CTA appears (onboarding Next/Get Started, primary actions).
 */
@Composable
fun GlassButton(
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    onClick: () -> Unit,
    content: @Composable () -> Unit
) {
    val shape = RoundedCornerShape(24.dp)

    val infinite = rememberInfiniteTransition(label = "button-shimmer")
    val shimmer by infinite.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 2800, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "button-shimmer-progress",
    )

    Box(
        modifier = modifier
            .graphicsLayer {
                shape = shape
                clip = true
            }
            .clip(shape)
            .background(
                brush = Brush.linearGradient(
                    colors = if (enabled) {
                        listOf(
                            SilverLight.copy(alpha = 0.45f),
                            GlassSilver.copy(alpha = 0.55f),
                            SilverLight.copy(alpha = 0.30f),
                        )
                    } else {
                        listOf(
                            Color.Gray.copy(alpha = 0.20f),
                            Color.Gray.copy(alpha = 0.30f),
                            Color.Gray.copy(alpha = 0.20f),
                        )
                    }
                )
            )
            .border(
                width = 1.dp,
                brush = Brush.linearGradient(
                    colors = if (enabled) {
                        listOf(
                            SilverLight.copy(alpha = 0.65f),
                            CrimsonRed.copy(alpha = 0.45f),
                            SilverLight.copy(alpha = 0.30f),
                        )
                    } else {
                        listOf(
                            Color.Gray.copy(alpha = 0.30f),
                            Color.Gray.copy(alpha = 0.20f),
                            Color.Gray.copy(alpha = 0.30f),
                        )
                    }
                ),
                shape = shape
            )
    ) {
        // Animated highlight sweep
        Box(
            modifier = Modifier
                .matchParentSize()
                .background(
                    brush = Brush.linearGradient(
                        colors = listOf(
                            Color.Transparent,
                            SilverLight.copy(alpha = 0.20f),
                            Color.Transparent,
                        ),
                        start = androidx.compose.ui.geometry.Offset(shimmer * 600f - 100f, 0f),
                        end = androidx.compose.ui.geometry.Offset(shimmer * 600f + 100f, 0f),
                    )
                )
        )
        Box(
            modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
            contentAlignment = androidx.compose.ui.Alignment.Center,
        ) {
            content()
        }
    }
}
