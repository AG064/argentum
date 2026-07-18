package com.argentum.data.update

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SemverComparisonTest {

    @Test
    fun `strict patch bump is newer`() {
        assertTrue(isRemoteNewer("0.0.9", "0.0.8"))
    }

    @Test
    fun `same version is not newer`() {
        assertFalse(isRemoteNewer("0.0.8", "0.0.8"))
    }

    @Test
    fun `older version is not newer`() {
        assertFalse(isRemoteNewer("0.0.7", "0.0.8"))
    }

    @Test
    fun `v prefix is ignored`() {
        assertTrue(isRemoteNewer("v0.0.9", "0.0.8"))
        assertTrue(isRemoteNewer("0.0.9", "v0.0.8"))
    }

    @Test
    fun `prerelease is older than the release`() {
        // 0.0.9-rc.1 is older than 0.0.9
        assertTrue(isRemoteNewer("0.0.9-rc.1", "0.0.8"))
        assertTrue(isRemoteNewer("0.0.9", "0.0.9-rc.1"))
    }

    @Test
    fun `major bump is newer`() {
        assertTrue(isRemoteNewer("1.0.0", "0.9.9"))
    }

    @Test
    fun `minor bump is newer`() {
        assertTrue(isRemoteNewer("0.1.0", "0.0.99"))
    }
}
