/**
 VersionValidatorTests

 Responsibilities:
 - Validate semantic version parsing logic.
 - Test version range validation.
 - Cover edge cases like invalid version strings.

 Does not handle:
 - Actual CLI execution (covered by RalphCLIClientTests).
 - UI integration (covered by E2E tests).
 */

import Foundation
import XCTest

@testable import RalphCore

final class VersionValidatorTests: XCTestCase {
    
    // MARK: - SemanticVersion Parsing
    
    func test_semanticVersion_parseSimple() {
        let version = VersionValidator.SemanticVersion(from: "0.1.0")
        XCTAssertNotNil(version)
        XCTAssertEqual(version?.major, 0)
        XCTAssertEqual(version?.minor, 1)
        XCTAssertEqual(version?.patch, 0)
    }
    
    func test_semanticVersion_parseWithPrefix() {
        let version = VersionValidator.SemanticVersion(from: "ralph 0.1.0")
        XCTAssertNotNil(version)
        XCTAssertEqual(version?.description, "0.1.0")
    }
    
    func test_semanticVersion_parseWithVPrefix() {
        let version = VersionValidator.SemanticVersion(from: "v0.2.5")
        XCTAssertNotNil(version)
        XCTAssertEqual(version?.description, "0.2.5")
    }
    
    func test_semanticVersion_parseWithNewlines() {
        let version = VersionValidator.SemanticVersion(from: "  0.1.0  \n")
        XCTAssertNotNil(version)
        XCTAssertEqual(version?.description, "0.1.0")
    }
    
    func test_semanticVersion_parseInvalid_returnsNil() {
        XCTAssertNil(VersionValidator.SemanticVersion(from: "invalid"))
        XCTAssertNil(VersionValidator.SemanticVersion(from: ""))
        XCTAssertNil(VersionValidator.SemanticVersion(from: "1.0"))  // Missing patch
        XCTAssertNil(VersionValidator.SemanticVersion(from: "1"))    // Missing minor/patch
    }
    
    // MARK: - SemanticVersion Comparison
    
    func test_semanticVersion_comparison() {
        let v1 = VersionValidator.SemanticVersion(major: 0, minor: 1, patch: 0)
        let v2 = VersionValidator.SemanticVersion(major: 0, minor: 1, patch: 1)
        let v3 = VersionValidator.SemanticVersion(major: 0, minor: 2, patch: 0)
        let v4 = VersionValidator.SemanticVersion(major: 1, minor: 0, patch: 0)
        
        XCTAssertTrue(v1 < v2)
        XCTAssertTrue(v2 < v3)
        XCTAssertTrue(v3 < v4)
        XCTAssertEqual(v1, VersionValidator.SemanticVersion(major: 0, minor: 1, patch: 0))
    }
    
    // MARK: - Version Validation
    
    func test_validate_currentVersion_compatible() {
        let validator = VersionValidator()
        let currentVersion = VersionCompatibility.minimumCLIVersion

        let result = validator.validate(currentVersion)
        XCTAssertTrue(result.isCompatible)
        if case .compatible = result.status { /* pass */ } else { XCTFail("Expected compatible") }
    }
    
    func test_validate_tooOld() {
        let validator = VersionValidator()
        
        let result = validator.validate("0.0.9")
        XCTAssertFalse(result.isCompatible)
        if case .tooOld(let found, let minimum) = result.status {
            XCTAssertEqual(found.description, "0.0.9")
            XCTAssertEqual(minimum.description, VersionCompatibility.minimumCLIVersion)
        } else {
            XCTFail("Expected tooOld status")
        }
        XCTAssertNotNil(result.errorMessage)
    }
    
    func test_validate_tooNew() {
        let validator = VersionValidator()
        let currentVersion = VersionValidator.SemanticVersion(from: VersionCompatibility.maximumCLIVersion)!
        let tooNewVersion = VersionValidator.SemanticVersion(
            major: currentVersion.major,
            minor: currentVersion.minor,
            patch: currentVersion.patch + 1
        )

        let result = validator.validate(tooNewVersion.description)
        XCTAssertFalse(result.isCompatible)
        if case .tooNew(let found, let maximum) = result.status {
            XCTAssertEqual(found.description, tooNewVersion.description)
            XCTAssertEqual(maximum.description, VersionCompatibility.maximumCLIVersion)
        } else {
            XCTFail("Expected tooNew status")
        }
        XCTAssertNotNil(result.errorMessage)
    }
    
    func test_validate_unparsable() {
        let validator = VersionValidator()
        
        let result = validator.validate("not a version")
        XCTAssertFalse(result.isCompatible)
        if case .unparsable(let raw) = result.status {
            XCTAssertEqual(raw, "not a version")
        } else {
            XCTFail("Expected unparsable status")
        }
        XCTAssertNotNil(result.errorMessage)
    }
    
    // MARK: - Error Messages
    
    func test_errorMessage_containsVersionInfo() {
        let validator = VersionValidator()
        let currentVersion = VersionValidator.SemanticVersion(from: VersionCompatibility.maximumCLIVersion)!
        let tooNewVersion = VersionValidator.SemanticVersion(
            major: currentVersion.major,
            minor: currentVersion.minor,
            patch: currentVersion.patch + 1
        )
        
        let tooOld = validator.validate("0.0.1")
        XCTAssertTrue(tooOld.errorMessage?.contains("0.0.1") ?? false)
        XCTAssertTrue(tooOld.errorMessage?.contains(VersionCompatibility.minimumCLIVersion) ?? false)
        
        let tooNew = validator.validate(tooNewVersion.description)
        XCTAssertTrue(tooNew.errorMessage?.contains(tooNewVersion.description) ?? false)
        XCTAssertTrue(tooNew.errorMessage?.contains(VersionCompatibility.maximumCLIVersion) ?? false)
    }
    
    func test_guidanceMessage_presentForErrors() {
        let validator = VersionValidator()
        
        let result = validator.validate("0.0.1")
        XCTAssertNotNil(result.guidanceMessage)
        XCTAssertTrue(result.guidanceMessage?.contains("reinstall") ?? false)
    }
    
    func test_guidanceMessage_nilForCompatible() {
        let validator = VersionValidator()

        let result = validator.validate(VersionCompatibility.minimumCLIVersion)
        XCTAssertNil(result.guidanceMessage)
        XCTAssertNil(result.errorMessage)
    }
    
    // MARK: - Version Compatibility Constants
    
    func test_versionCompatibility_constantsAreValidSemver() {
        let minVersion = VersionValidator.SemanticVersion(from: VersionCompatibility.minimumCLIVersion)
        let maxVersion = VersionValidator.SemanticVersion(from: VersionCompatibility.maximumCLIVersion)
        
        XCTAssertNotNil(minVersion, "Minimum CLI version should be valid semantic version")
        XCTAssertNotNil(maxVersion, "Maximum CLI version should be valid semantic version")
        
        // Maximum should be >= minimum
        if let min = minVersion, let max = maxVersion {
            XCTAssertTrue(max >= min, "Maximum version should be >= minimum version")
        }
    }
    
    func test_versionCompatibility_defaultValidatorMatchesConstants() {
        let validator = VersionValidator()
        let minVersion = VersionValidator.SemanticVersion(from: VersionCompatibility.minimumCLIVersion)!
        let maxVersion = VersionValidator.SemanticVersion(from: VersionCompatibility.maximumCLIVersion)!
        
        // Test that minimum version is accepted
        let minResult = validator.validate(minVersion.description)
        XCTAssertTrue(minResult.isCompatible, "Minimum CLI version should be compatible")
        
        // Test that maximum version is accepted
        let maxResult = validator.validate(maxVersion.description)
        XCTAssertTrue(maxResult.isCompatible, "Maximum CLI version should be compatible")
    }
}
